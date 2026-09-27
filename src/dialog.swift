import AppKit
import CoreGraphics

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let arguments = CommandLine.arguments

if arguments.dropFirst().first == "error" {
    let alert = NSAlert()
    alert.messageText = "Screenlink could not start"
    alert.informativeText = arguments.dropFirst(2).first ?? ""
    alert.addButton(withTitle: "OK")
    app.activate(ignoringOtherApps: true)
    alert.runModal()
} else {
    let address = arguments.count > 2 ? arguments[2] : ""
    let privateData = String(data: FileHandle.standardInput.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let parts = privateData.split(separator: "\n", omittingEmptySubsequences: false)
    let code = parts.first.map(String.init) ?? ""
    let fingerprint = parts.dropFirst().first.map(String.init) ?? ""

    final class Controls: NSObject, NSWindowDelegate {
        let address: String
        var window: NSWindow?
        init(address: String) { self.address = address }
        @objc func copyAddress(_ sender: Any?) {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(address, forType: .string)
        }
        @objc func allowRecording(_ sender: Any?) { _ = CGRequestScreenCaptureAccess() }
        @objc func allowControl(_ sender: Any?) {
            if !CGRequestPostEventAccess(),
               let settings = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
                NSWorkspace.shared.open(settings)
            }
        }
        @objc func stop(_ sender: Any?) { NSApp.terminate(nil) }
        func windowWillClose(_ notification: Notification) { NSApp.terminate(nil) }
    }

    let controls = Controls(address: address)
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 520, height: 320),
        styleMask: [.titled, .closable, .miniaturizable],
        backing: .buffered,
        defer: false
    )
    window.title = "Screenlink"
    window.center()
    window.isReleasedWhenClosed = false
    window.delegate = controls
    controls.window = window

    func label(_ text: String, font: NSFont? = nil) -> NSTextField {
        let field = NSTextField(labelWithString: text)
        field.isSelectable = true
        if let font { field.font = font }
        return field
    }
    func button(_ title: String, _ action: Selector) -> NSButton {
        NSButton(title: title, target: controls, action: action)
    }

    let stack = NSStackView()
    stack.orientation = .vertical
    stack.alignment = .leading
    stack.spacing = 12
    stack.addArrangedSubview(label("Open this address on your other device:", font: .boldSystemFont(ofSize: 14)))
    stack.addArrangedSubview(label(address, font: .monospacedSystemFont(ofSize: 15, weight: .regular)))
    stack.addArrangedSubview(label("Session code: \(code)", font: .monospacedDigitSystemFont(ofSize: 20, weight: .semibold)))
    stack.addArrangedSubview(label("Keep this window open while sharing. Closing it stops Screenlink."))
    stack.addArrangedSubview(label("For remote control, turn on Screenlink in Accessibility settings."))
    stack.addArrangedSubview(label("TLS certificate SHA-256 (compare with the browser warning):"))
    stack.addArrangedSubview(label(fingerprint, font: .monospacedSystemFont(ofSize: 10, weight: .regular)))

    let actions = NSStackView(views: [
        button("Copy address", #selector(Controls.copyAddress(_:))),
        button("Allow Screen Recording", #selector(Controls.allowRecording(_:))),
        button("Allow Remote Control", #selector(Controls.allowControl(_:)))
    ])
    actions.orientation = .horizontal
    actions.spacing = 8
    stack.addArrangedSubview(actions)
    stack.addArrangedSubview(button("Stop Sharing", #selector(Controls.stop(_:))))

    let content = NSView()
    window.contentView = content
    content.addSubview(stack)
    stack.translatesAutoresizingMaskIntoConstraints = false
    NSLayoutConstraint.activate([
        stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 24),
        stack.trailingAnchor.constraint(lessThanOrEqualTo: content.trailingAnchor, constant: -24),
        stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 24),
        stack.bottomAnchor.constraint(lessThanOrEqualTo: content.bottomAnchor, constant: -24)
    ])
    window.makeKeyAndOrderFront(nil)
    app.activate(ignoringOtherApps: true)
    app.run()
}
