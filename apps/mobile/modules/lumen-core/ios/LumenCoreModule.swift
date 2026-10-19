import ExpoModulesCore

/// The Rust core, as JavaScript sees it (ADR-0007). Every call goes through the
/// UniFFI bindings; JavaScript gets plain values, never handles or paths to act on.
public class LumenCoreModule: Module {
  public func definition() -> ModuleDefinition {
    Name("LumenCore")

    // The version of the Rust core this app loaded.
    Function("coreVersion") {
      coreVersion()
    }
  }
}
