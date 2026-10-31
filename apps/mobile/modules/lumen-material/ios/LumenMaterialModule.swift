import ExpoModulesCore
import UIKit

public class LumenMaterialModule: Module {
  public func definition() -> ModuleDefinition {
    Name("LumenMaterial")
    View(MaterialView.self) {}
  }
}

/// Background only: React's text and controls stay sharp siblings above it.
final class MaterialView: ExpoView {
  private let material: UIVisualEffectView

  required init(appContext: AppContext? = nil) {
    let effect: UIVisualEffect
    if #available(iOS 26.0, *) {
      effect = UIGlassEffect(style: .regular)
    } else {
      effect = UIBlurEffect(style: .systemThinMaterial)
    }
    material = UIVisualEffectView(effect: effect)
    super.init(appContext: appContext)
    isUserInteractionEnabled = false
    material.isUserInteractionEnabled = false
    addSubview(material)
    // UIKit's semantic material follows appearance, contrast and reduced
    // transparency automatically. Leave alpha at 1 for correct composition.
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    material.frame = bounds
  }
}
