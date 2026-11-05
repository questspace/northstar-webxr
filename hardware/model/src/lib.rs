//! Steady-state estimates, not a battery discharge curve or USB electrical simulation.

#[no_mangle]
pub extern "C" fn source_watts(load_w: f64, supply_v: f64, loop_ohm: f64, efficiency: f64) -> f64 {
    if ![load_w, supply_v, loop_ohm, efficiency].iter().all(|v| v.is_finite())
        || load_w < 0.0 || supply_v <= 0.0 || loop_ohm < 0.0
        || efficiency <= 0.0 || efficiency > 1.0 {
        return f64::NAN;
    }
    let input_w = load_w / efficiency;
    let discriminant = supply_v * supply_v - 4.0 * loop_ohm * input_w;
    if discriminant <= 0.0 { return f64::NAN; }
    let current = 2.0 * input_w / (supply_v + discriminant.sqrt());
    supply_v * current
}

#[no_mangle]
pub extern "C" fn runtime_hours(energy_wh: f64, usable: f64, bank_efficiency: f64, source_w: f64) -> f64 {
    if ![energy_wh, usable, bank_efficiency, source_w].iter().all(|v| v.is_finite())
        || energy_wh <= 0.0 || source_w <= 0.0 || usable <= 0.0 || usable > 1.0
        || bank_efficiency <= 0.0 || bank_efficiency > 1.0 {
        return f64::NAN;
    }
    energy_wh * usable * bank_efficiency / source_w
}

#[no_mangle]
pub extern "C" fn moment_nm(mass_g: f64, offset_mm: f64) -> f64 {
    if !mass_g.is_finite() || mass_g < 0.0 || !offset_mm.is_finite() { return f64::NAN; }
    mass_g * offset_mm * 9.80665 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cable_and_converter_conserve_energy() {
        let source = source_watts(10.0, 12.0, 0.4, 0.9);
        let current = source / 12.0;
        assert!((source - current * current * 0.4 - 10.0 / 0.9).abs() < 1e-10);
        assert!(source_watts(10.0, 5.0, 1.0, 1.0).is_nan());
    }
    #[test]
    fn runtime_and_balance_use_physical_units() {
        assert_eq!(runtime_hours(100.0, 0.8, 0.9, 24.0), 3.0);
        assert!((moment_nm(200.0, 50.0) + moment_nm(100.0, -100.0)).abs() < 1e-12);
        assert_eq!(source_watts(9.0, 12.0, 0.0, 0.9), 10.0);
    }
    #[test]
    fn invalid_inputs_are_not_predictions() {
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(source_watts(bad, 12.0, 0.2, 0.9).is_nan());
            assert!(runtime_hours(bad, 0.9, 0.9, 20.0).is_nan());
        }
        assert!(runtime_hours(100.0, 1.1, 0.9, 20.0).is_nan());
        assert!(source_watts(10.0, 12.0, 0.2, 0.0).is_nan());
    }
}
