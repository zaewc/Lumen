# The Rust core for iOS (ADR-0007, roadmap 11.15): LumenCore.xcframework and
# the UniFFI Swift bindings, which `cargo xtask ios` copies into ./rust. Run
# that command before `pod install`.
Pod::Spec.new do |s|
  s.name           = 'LumenCore'
  s.version        = '0.0.0'
  s.summary        = "Lumen's Rust core as an Expo module."
  s.license        = 'Apache-2.0'
  s.author         = 'Lumen'
  s.homepage       = 'https://github.com/zaewc/Lumen'
  s.platforms      = { :ios => '16.4' }
  s.swift_version  = '5.9'
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'
  s.frameworks = 'Photos', 'Vision', 'BackgroundTasks'

  s.source_files = '*.swift', 'rust/lumen_ffi.swift'
  s.vendored_frameworks = 'rust/LumenCore.xcframework'
end
