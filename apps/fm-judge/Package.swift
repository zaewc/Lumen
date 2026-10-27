// swift-tools-version: 6.2
// The on-device judge helper for evaluation (ADR-0044): asks Apple's
// on-device Foundation Models the Decisions questions about one redacted
// `jev.request/1` per line and answers with enumerated values only.
import PackageDescription

let package = Package(
  name: "fm-judge",
  platforms: [.macOS(.v26)],
  products: [
    .executable(name: "fm-judge", targets: ["fm-judge"])
  ],
  targets: [
    .target(name: "FMJudgeCore"),
    .executableTarget(name: "fm-judge", dependencies: ["FMJudgeCore"]),
    .testTarget(name: "FMJudgeCoreTests", dependencies: ["FMJudgeCore"]),
  ]
)
