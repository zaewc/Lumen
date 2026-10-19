import ExpoModulesCore

/// The Rust core, as JavaScript sees it (ADR-0007). Every call goes through the
/// UniFFI bindings; JavaScript gets plain values, never handles or paths to act on.
public class LumenCoreModule: Module {
  private let analyzer = Analyzer()

  public func definition() -> ModuleDefinition {
    Name("LumenCore")

    // The version of the Rust core this app loaded.
    Function("coreVersion") {
      coreVersion()
    }

    // Groups of near-duplicates among the newest `limit` photos (spike).
    AsyncFunction("analyze") { (limit: Int, threshold: Double, promise: Promise) in
      self.analyzer.run(limit: limit, threshold: Float(threshold)) { result in
        switch result {
        case .success(let analysis):
          promise.resolve(analysis)
        case .failure(let error):
          promise.reject("ERR_ANALYZE", error.localizedDescription)
        }
      }
    }
  }
}
