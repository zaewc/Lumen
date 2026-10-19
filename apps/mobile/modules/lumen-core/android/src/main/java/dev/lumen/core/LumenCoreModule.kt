package dev.lumen.core

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * The Rust core, as JavaScript sees it (ADR-0007). Every call goes through the
 * UniFFI bindings; JavaScript gets plain values, never handles or paths to act on.
 */
class LumenCoreModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("LumenCore")

    // The version of the Rust core this app loaded.
    Function("coreVersion") { uniffi.lumen_ffi.coreVersion() }
  }
}
