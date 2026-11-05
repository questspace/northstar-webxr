import Foundation
import RealityKit

@main
struct Reconstruct {
    static func main() async throws {
        guard CommandLine.arguments.count == 3 else {
            print("Usage: reconstruct INPUT_IMAGES OUTPUT_DIRECTORY")
            exit(2)
        }
        guard PhotogrammetrySession.isSupported else {
            print("Object Capture is not supported on this Mac")
            exit(1)
        }
        let input = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
        let output = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        var configuration = PhotogrammetrySession.Configuration()
        configuration.sampleOrdering = .sequential
        configuration.featureSensitivity = .high
        configuration.isObjectMaskingEnabled = true
        let session = try PhotogrammetrySession(input: input, configuration: configuration)
        let requests: [PhotogrammetrySession.Request] = [
            .modelFile(url: output.appendingPathComponent("headset.usdz"), detail: .medium),
            .modelFile(url: output.appendingPathComponent("obj", isDirectory: true), detail: .medium)
        ]
        try session.process(requests: requests)
        var lastPercent = -1
        var failed = false
        for try await event in session.outputs {
            switch event {
            case .requestProgress(_, let fraction):
                let percent = Int(fraction * 100)
                if percent != lastPercent { print("Progress: \(percent)%"); lastPercent = percent }
            case .requestComplete(_, let result): print("Completed: \(result)")
            case .requestError(_, let error): print("Request failed: \(error)"); failed = true
            case .processingComplete: exit(failed ? 1 : 0)
            case .inputComplete: print("Images ingested")
            case .invalidSample(let id, let reason): print("Invalid sample \(id): \(reason)")
            case .skippedSample(let id): print("Skipped sample \(id)")
            case .automaticDownsampling: print("Automatic downsampling")
            case .processingCancelled: exit(1)
            default: break
            }
            fflush(stdout)
        }
    }
}
