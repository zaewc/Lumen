import ExpoModulesCore
import Photos

/// The photo library for JavaScript (ADR-0007): an access request, the number
/// of images, and pages of them. Pages hold identifiers and display URIs only;
/// nothing is downloaded from iCloud and nothing is changed.
public class LumenPhotosModule: Module {
  private var images: PHFetchResult<PHAsset>?

  private func fetch() -> PHFetchResult<PHAsset> {
    if let images {
      return images
    }
    let options = PHFetchOptions()
    options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
    let result = PHAsset.fetchAssets(with: .image, options: options)
    images = result
    return result
  }

  private static func text(_ status: PHAuthorizationStatus) -> String {
    switch status {
    case .authorized: return "granted"
    case .limited: return "limited"
    case .denied, .restricted: return "denied"
    case .notDetermined: return "undetermined"
    @unknown default: return "denied"
    }
  }

  public func definition() -> ModuleDefinition {
    Name("LumenPhotos")

    AsyncFunction("requestAccess") { (promise: Promise) in
      PHPhotoLibrary.requestAuthorization(for: .readWrite) { status in
        self.images = nil
        promise.resolve(Self.text(status))
      }
    }

    Function("count") { () -> Int in
      self.fetch().count
    }

    Function("page") { (offset: Int, limit: Int) -> [[String: Any]] in
      let result = self.fetch()
      guard offset >= 0, limit > 0, offset < result.count else {
        return []
      }
      let end = min(result.count, offset + limit)
      var page: [[String: Any]] = []
      page.reserveCapacity(end - offset)
      result.enumerateObjects(at: IndexSet(integersIn: offset..<end)) { asset, _, _ in
        page.append([
          "id": asset.localIdentifier,
          "uri": "ph://\(asset.localIdentifier)",
          "width": asset.pixelWidth,
          "height": asset.pixelHeight,
        ])
      }
      return page
    }
  }
}
