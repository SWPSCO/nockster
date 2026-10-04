import UIKit
import SwiftUI
import Capacitor

@MainActor
class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    private let model = WalletModel()
    private var privacyCover: UIView?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = UIHostingController(rootView: WalletRootView(model: model))
        self.window = window
        window.makeKeyAndVisible()
        for context in options.urlContexts { model.receive(context.url) }
        SceneDelegateProxy.shared.scene(scene, willConnectTo: session, options: options)
    }

    func sceneWillResignActive(_ scene: UIScene) {
        if let window, privacyCover == nil {
            let cover = UIView(frame: window.bounds)
            cover.backgroundColor = .systemBackground
            cover.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            window.addSubview(cover)
            privacyCover = cover
        }
        model.setActive(false)
    }

    func sceneDidEnterBackground(_ scene: UIScene) { model.enterBackground() }

    func sceneDidBecomeActive(_ scene: UIScene) {
        model.setActive(true)
        privacyCover?.removeFromSuperview()
        privacyCover = nil
    }

    func scene(_ scene: UIScene, openURLContexts contexts: Set<UIOpenURLContext>) {
        for context in contexts { model.receive(context.url) }
        SceneDelegateProxy.shared.scene(scene, openURLContexts: contexts)
    }
}
