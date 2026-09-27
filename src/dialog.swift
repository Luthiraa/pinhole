import AppKit
import CoreGraphics

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let arguments = CommandLine.arguments

if arguments.dropFirst().first == "error" {
    let alert = NSAlert()
    alert.messageText = "Pinhole could not start"
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

    final class MarkView: NSView {
        override func draw(_ dirtyRect: NSRect) {
            NSColor.labelColor.setFill()
            NSBezierPath(roundedRect: bounds, xRadius: 9, yRadius: 9).fill()
            let outer = NSRect(x: bounds.midX - 8, y: bounds.midY - 8, width: 16, height: 16)
            NSColor.windowBackgroundColor.setFill()
            NSBezierPath(ovalIn: outer).fill()
            let inner = NSRect(x: bounds.midX - 3, y: bounds.midY - 3, width: 6, height: 6)
            NSColor.labelColor.setFill()
            NSBezierPath(ovalIn: inner).fill()
        }
    }

    func text(_ value: String, size: CGFloat = 13, weight: NSFont.Weight = .regular,
              color: NSColor = .labelColor, mono: Bool = false) -> NSTextField {
        let field = NSTextField(labelWithString: value)
        field.font = mono ? .monospacedSystemFont(ofSize: size, weight: weight) : .systemFont(ofSize: size, weight: weight)
        field.textColor = color
        field.isSelectable = true
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return field
    }

    func stack(_ views: [NSView], spacing: CGFloat = 8, vertical: Bool = true) -> NSStackView {
        let result = NSStackView(views: views)
        result.orientation = vertical ? .vertical : .horizontal
        result.alignment = vertical ? .leading : .centerY
        result.spacing = spacing
        return result
    }

    func card(_ content: NSView) -> NSView {
        let view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        view.layer?.borderColor = NSColor.separatorColor.cgColor
        view.layer?.borderWidth = 1
        view.layer?.cornerRadius = 10
        view.addSubview(content)
        content.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
            content.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
            content.topAnchor.constraint(equalTo: view.topAnchor, constant: 14),
            content.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -14)
        ])
        return view
    }

    func button(_ title: String, _ action: Selector, controls: Controls) -> NSButton {
        let result = NSButton(title: title, target: controls, action: action)
        result.bezelStyle = .rounded
        result.controlSize = .large
        result.font = .systemFont(ofSize: 13, weight: .medium)
        return result
    }

    let controls = Controls(address: address)
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 600, height: 510),
        styleMask: [.titled, .closable, .miniaturizable],
        backing: .buffered,
        defer: false
    )
    window.title = "Pinhole"
    window.titleVisibility = .hidden
    window.center()
    window.isReleasedWhenClosed = false
    window.delegate = controls

    let mark = MarkView(frame: NSRect(x: 0, y: 0, width: 34, height: 34))
    mark.widthAnchor.constraint(equalToConstant: 34).isActive = true
    mark.heightAnchor.constraint(equalToConstant: 34).isActive = true
    let heading = stack([
        text("Pinhole", size: 22, weight: .semibold),
        text("Private control for your Mac", size: 12, color: .secondaryLabelColor)
    ], spacing: 1)
    let sharing = text("●  SHARING", size: 10, weight: .semibold, color: .systemGreen)
    let spacer = NSView()
    spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
    let header = stack([mark, heading, spacer, sharing], spacing: 12, vertical: false)

    let copy = button("Copy address", #selector(Controls.copyAddress(_:)), controls: controls)
    let addressField = text(address, size: 15, weight: .medium, mono: true)
    addressField.lineBreakMode = .byTruncatingMiddle
    let addressRow = stack([addressField, copy], spacing: 12, vertical: false)
    let details = card(stack([
        text("OPEN ON YOUR OTHER DEVICE", size: 10, weight: .semibold, color: .secondaryLabelColor),
        addressRow,
        text("SESSION CODE", size: 10, weight: .semibold, color: .secondaryLabelColor),
        text(code, size: 29, weight: .semibold, mono: true)
    ], spacing: 9))

    let permissions = stack([
        text("Permissions", size: 15, weight: .semibold),
        text("Allow Pinhole to show and control this Mac.", size: 12, color: .secondaryLabelColor),
        stack([
            button("Screen Recording", #selector(Controls.allowRecording(_:)), controls: controls),
            button("Remote Control", #selector(Controls.allowControl(_:)), controls: controls)
        ], spacing: 10, vertical: false)
    ], spacing: 7)

    let certificate = card(stack([
        text("CERTIFICATE SHA-256", size: 10, weight: .semibold, color: .secondaryLabelColor),
        text(fingerprint, size: 10, mono: true),
        text("Compare this with your browser's certificate details before continuing.", size: 11, color: .secondaryLabelColor)
    ], spacing: 6))

    let stop = button("Stop sharing", #selector(Controls.stop(_:)), controls: controls)
    stop.bezelColor = .labelColor
    stop.contentTintColor = .windowBackgroundColor
    let footerSpacer = NSView()
    footerSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
    let footer = stack([
        text("Closing this window ends the session.", size: 12, color: .secondaryLabelColor),
        footerSpacer, stop
    ], spacing: 10, vertical: false)

    let root = stack([header, details, permissions, certificate, footer], spacing: 20)
    let content = NSView()
    window.contentView = content
    content.addSubview(root)
    root.translatesAutoresizingMaskIntoConstraints = false
    NSLayoutConstraint.activate([
        root.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 26),
        root.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -26),
        root.topAnchor.constraint(equalTo: content.topAnchor, constant: 24),
        root.bottomAnchor.constraint(lessThanOrEqualTo: content.bottomAnchor, constant: -24),
        header.widthAnchor.constraint(equalTo: root.widthAnchor),
        details.widthAnchor.constraint(equalTo: root.widthAnchor),
        certificate.widthAnchor.constraint(equalTo: root.widthAnchor),
        footer.widthAnchor.constraint(equalTo: root.widthAnchor)
    ])
    window.makeKeyAndOrderFront(nil)
    app.activate(ignoringOtherApps: true)
    app.run()
}
