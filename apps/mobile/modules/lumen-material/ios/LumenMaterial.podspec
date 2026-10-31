Pod::Spec.new do |s|
  s.name = 'LumenMaterial'
  s.version = '0.0.0'
  s.summary = "Lumen's adaptive interface material."
  s.license = 'Apache-2.0'
  s.author = 'Lumen'
  s.homepage = 'https://github.com/zaewc/Lumen'
  s.platforms = { :ios => '16.4' }
  s.swift_version = '5.9'
  s.source = { git: '' }
  s.static_framework = true
  s.dependency 'ExpoModulesCore'
  s.frameworks = 'UIKit'
  s.source_files = '*.swift'
end
