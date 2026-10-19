# The photo library, enumerated natively for the app (ADR-0007): PhotoKit.
Pod::Spec.new do |s|
  s.name           = 'LumenPhotos'
  s.version        = '0.0.0'
  s.summary        = "Lumen's photo library adapter as an Expo module."
  s.license        = 'Apache-2.0'
  s.author         = 'Lumen'
  s.homepage       = 'https://github.com/zaewc/Lumen'
  s.platforms      = { :ios => '16.4' }
  s.swift_version  = '5.9'
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'
  s.frameworks = 'Photos'

  s.source_files = '*.swift'
end
