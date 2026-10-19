import BackgroundTasks
import Foundation
import Photos
import UIKit
import Vision

/// Finds near-duplicate photos (ADR-0007's spike, ADR-0019): small PhotoKit
/// thumbnails (never iCloud originals) → Vision feature prints → grouping in
/// Rust. On iOS 26+ the work runs as a continued background task the person
/// started, with a Live Activity showing progress; earlier versions run it on
/// a background queue.
final class Analyzer {
  static let taskIdentifier = "dev.lumen.mobile.analyze"

  /// A flag shared with the expiration handler.
  private final class Flag {
    private let lock = NSLock()
    private var value = false
    func set() { lock.lock(); value = true; lock.unlock() }
    func get() -> Bool { lock.lock(); defer { lock.unlock() }; return value }
  }

  private struct Job {
    let limit: Int
    let threshold: Float
    let done: (Result<[String: Any], Error>) -> Void
  }

  private let lock = NSLock()
  private var job: Job?
  private var registered = false

  /// Analyzes the newest `limit` photos; `done` gets the groups and timings.
  func run(limit: Int, threshold: Float, done: @escaping (Result<[String: Any], Error>) -> Void) {
    lock.lock()
    job = Job(limit: limit, threshold: threshold, done: done)
    lock.unlock()
    if #available(iOS 26.0, *), submitContinued() {
      return
    }
    DispatchQueue.global(qos: .userInitiated).async {
      self.finish(self.analyze(limit: limit, threshold: threshold, progress: nil, expired: Flag()))
    }
  }

  private func finish(_ result: Result<[String: Any], Error>) {
    lock.lock()
    let current = job
    job = nil
    lock.unlock()
    current?.done(result)
  }

  /// Registers (once) and submits the continued task; false if the system
  /// refused, so the caller runs the work itself.
  @available(iOS 26.0, *)
  private func submitContinued() -> Bool {
    let scheduler = BGTaskScheduler.shared
    if !registered {
      registered = scheduler.register(forTaskWithIdentifier: Self.taskIdentifier, using: nil) { task in
        guard let task = task as? BGContinuedProcessingTask else {
          return
        }
        self.lock.lock()
        let current = self.job
        self.lock.unlock()
        guard let current else {
          task.setTaskCompleted(success: false)
          return
        }
        let expired = Flag()
        task.expirationHandler = { expired.set() }
        let result = self.analyze(
          limit: current.limit, threshold: current.threshold, progress: task.progress, expired: expired)
        task.setTaskCompleted(success: (try? result.get()) != nil)
        self.finish(result)
      }
    }
    let request = BGContinuedProcessingTaskRequest(
      identifier: Self.taskIdentifier,
      title: "Finding similar photos",
      subtitle: "Comparing small previews on this device"
    )
    do {
      try scheduler.submit(request)
      return true
    } catch {
      return false
    }
  }

  private func featurePrint(_ image: CGImage) -> [Float]? {
    let request = VNGenerateImageFeaturePrintRequest()
    let handler = VNImageRequestHandler(cgImage: image, options: [:])
    guard (try? handler.perform([request])) != nil,
      let observation = request.results?.first,
      observation.elementType == .float
    else {
      return nil
    }
    return observation.data.withUnsafeBytes { Array($0.bindMemory(to: Float.self)) }
  }

  private func analyze(
    limit: Int, threshold: Float, progress: Progress?, expired: Flag
  ) -> Result<[String: Any], Error> {
    let options = PHFetchOptions()
    options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
    options.fetchLimit = max(0, limit)
    let assets = PHAsset.fetchAssets(with: .image, options: options)
    let images = PHImageManager.default()
    let request = PHImageRequestOptions()
    request.isSynchronous = true
    request.deliveryMode = .highQualityFormat
    request.resizeMode = .fast
    request.isNetworkAccessAllowed = false
    progress?.totalUnitCount = Int64(assets.count)

    var ids: [String] = []
    var prints: [[Float]] = []
    let started = Date()
    for index in 0..<assets.count {
      if expired.get() {
        break
      }
      autoreleasepool {
        let asset = assets[index]
        var thumbnail: CGImage?
        images.requestImage(
          for: asset, targetSize: CGSize(width: 299, height: 299), contentMode: .aspectFill,
          options: request
        ) { image, _ in thumbnail = image?.cgImage }
        if let thumbnail, let print = featurePrint(thumbnail) {
          ids.append(asset.localIdentifier)
          prints.append(print)
        }
      }
      progress?.completedUnitCount = Int64(index + 1)
    }
    let printed = Date()
    do {
      let groups = try groupFeaturePrints(prints: prints, threshold: threshold)
      var members: [UInt32: [String]] = [:]
      for (id, group) in zip(ids, groups) {
        members[group, default: []].append(id)
      }
      return .success([
        "analyzed": ids.count,
        "groups": members.values.filter { $0.count > 1 },
        "printMs": Int(printed.timeIntervalSince(started) * 1000),
        "groupMs": Int(Date().timeIntervalSince(printed) * 1000),
        "cancelled": expired.get(),
      ])
    } catch {
      return .failure(error)
    }
  }
}
