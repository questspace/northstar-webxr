//! Extended Unified Camera Model (Khomenko et al. 2016): a closed-form fisheye model with a
//! closed-form inverse. The XR50 stores its own model on the device; we fit EUCM to rays the
//! official SDK traced through it (`xvsdk-bridge --dump-calibration`).

/// fx, fy, cx, cy in pixels; alpha in [0,1]; beta > 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eucm {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub alpha: f64,
    pub beta: f64,
}

impl Eucm {
    fn from_params(p: [f64; 6]) -> Eucm {
        Eucm { fx: p[0], fy: p[1], cx: p[2], cy: p[3], alpha: p[4], beta: p[5] }
    }

    /// Camera-frame point (x right, y down, z forward) → pixel; None behind the model's horizon.
    pub fn project(&self, p: [f64; 3]) -> Option<[f64; 2]> {
        let [x, y, z] = p;
        let d = (self.beta * (x * x + y * y) + z * z).sqrt();
        let den = self.alpha * d + (1.0 - self.alpha) * z;
        if den <= 1e-9 {
            return None;
        }
        Some([self.fx * x / den + self.cx, self.fy * y / den + self.cy])
    }

    /// Pixel → unit ray in the camera frame.
    pub fn unproject(&self, uv: [f64; 2]) -> Option<[f64; 3]> {
        let mx = (uv[0] - self.cx) / self.fx;
        let my = (uv[1] - self.cy) / self.fy;
        let r2 = mx * mx + my * my;
        let disc = 1.0 - (2.0 * self.alpha - 1.0) * self.beta * r2;
        if disc < 0.0 {
            return None;
        }
        let mz = (1.0 - self.beta * self.alpha * self.alpha * r2) / (self.alpha * disc.sqrt() + 1.0 - self.alpha);
        let n = (mx * mx + my * my + mz * mz).sqrt();
        Some([mx / n, my / n, mz / n])
    }

    /// Least-squares fit (Levenberg-Marquardt, numeric Jacobian) of the model to
    /// `(pixel, ray)` pairs, minimising reprojection error. Returns the model and its RMS
    /// error in pixels.
    pub fn fit(samples: &[([f64; 2], [f64; 3])], width: f64, height: f64) -> (Eucm, f64) {
        let residuals = |p: &[f64; 6]| -> Vec<f64> {
            let m = Eucm::from_params(*p);
            samples
                .iter()
                .flat_map(|(uv, ray)| match m.project(*ray) {
                    Some(q) => [q[0] - uv[0], q[1] - uv[1]],
                    None => [1e3, 1e3],
                })
                .collect()
        };
        let cost = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
        let mut p = [width / 3.0, width / 3.0, width / 2.0, height / 2.0, 0.6, 1.0];
        let mut r = residuals(&p);
        let mut lambda = 1e-3;
        for _ in 0..200 {
            // J^T J and J^T r with central differences
            let mut jtj = [[0.0; 6]; 6];
            let mut jtr = [0.0; 6];
            let cols: Vec<Vec<f64>> = (0..6)
                .map(|k| {
                    let h = 1e-6 * p[k].abs().max(1e-3);
                    let (mut a, mut b) = (p, p);
                    a[k] += h;
                    b[k] -= h;
                    residuals(&a).iter().zip(residuals(&b)).map(|(x, y)| (x - y) / (2.0 * h)).collect()
                })
                .collect();
            for i in 0..6 {
                jtr[i] = cols[i].iter().zip(&r).map(|(j, e)| j * e).sum();
                for k in 0..6 {
                    jtj[i][k] = cols[i].iter().zip(&cols[k]).map(|(a, b)| a * b).sum();
                }
            }
            let mut a = jtj;
            for (i, row) in a.iter_mut().enumerate() {
                row[i] *= 1.0 + lambda;
            }
            let Some(step) = solve6(a, jtr) else { break };
            let mut q = p;
            for i in 0..6 {
                q[i] -= step[i];
            }
            q[4] = q[4].clamp(0.0, 1.0);
            q[5] = q[5].max(1e-6);
            let rq = residuals(&q);
            if cost(&rq) < cost(&r) {
                let gain = cost(&r) - cost(&rq);
                p = q;
                r = rq;
                lambda = (lambda * 0.3).max(1e-12);
                if gain < 1e-12 {
                    break;
                }
            } else {
                lambda *= 10.0;
            }
        }
        let rms = (cost(&r) / samples.len() as f64).sqrt();
        (Eucm::from_params(p), rms)
    }
}

/// Solves a 6x6 linear system by Gaussian elimination with partial pivoting.
fn solve6(mut a: [[f64; 6]; 6], mut b: [f64; 6]) -> Option<[f64; 6]> {
    for c in 0..6 {
        let pivot = (c..6).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[pivot][c].abs() < 1e-15 {
            return None;
        }
        a.swap(c, pivot);
        b.swap(c, pivot);
        for r in c + 1..6 {
            let f = a[r][c] / a[c][c];
            for k in c..6 {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; 6];
    for r in (0..6).rev() {
        x[r] = (b[r] - (r + 1..6).map(|k| a[r][k] * x[k]).sum::<f64>()) / a[r][r];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_fit() {
        let truth = Eucm { fx: 220.0, fy: 221.0, cx: 318.0, cy: 202.0, alpha: 0.62, beta: 1.08 };
        let samples: Vec<_> = (0..25)
            .flat_map(|v| (0..40).map(move |u| [u as f64 * 16.0, v as f64 * 16.0]))
            .filter_map(|uv| truth.unproject(uv).map(|ray| (uv, ray)))
            .collect();
        for (uv, ray) in &samples {
            let back = truth.project(*ray).unwrap();
            assert!((back[0] - uv[0]).abs() < 1e-6 && (back[1] - uv[1]).abs() < 1e-6);
        }
        let (fit, rms) = Eucm::fit(&samples, 640.0, 400.0);
        assert!(rms < 1e-3, "rms {rms}");
        assert!((fit.fx - truth.fx).abs() < 0.01 && (fit.alpha - truth.alpha).abs() < 1e-4, "{fit:?}");
    }
}
