import Cocoa

/// A custom NSView for displaying a task in a menu with the task name on the left
/// and a right-aligned time on the right.
class TaskMenuItemView: NSView {
    
    // MARK: - Constants
    
    /// Standard menu item height
    private static let menuItemHeight: CGFloat = 22
    
    /// Standard menu width for status bar menus
    private static let menuWidth: CGFloat = 280
    
    /// Left margin for content
    private let leftMargin: CGFloat = 20
    
    /// Right margin for content
    private let rightMargin: CGFloat = 12
    
    /// Spacing between title and time
    private let horizontalSpacing: CGFloat = 8
    
    // MARK: - Views
    
    private lazy var titleLabel: NSTextField = {
        let field = NSTextField(labelWithString: "")
        field.translatesAutoresizingMaskIntoConstraints = false
        field.lineBreakMode = .byTruncatingTail
        field.font = NSFont.menuFont(ofSize: 0)
        field.textColor = NSColor.controlTextColor
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return field
    }()
    
    private lazy var timeLabel: NSTextField = {
        let field = NSTextField(labelWithString: "")
        field.translatesAutoresizingMaskIntoConstraints = false
        field.font = NSFont.menuFont(ofSize: 0)
        field.textColor = NSColor.secondaryLabelColor
        field.alignment = .right
        field.setContentHuggingPriority(.required, for: .horizontal)
        return field
    }()
    
    private lazy var highlightView: NSVisualEffectView = {
        let view = NSVisualEffectView()
        view.translatesAutoresizingMaskIntoConstraints = false
        view.state = .active
        view.material = .selection
        view.blendingMode = .behindWindow
        view.isEmphasized = true
        view.wantsLayer = true
        view.isHidden = true
        return view
    }()
    
    // MARK: - Initialization
    
    init(title: String, time: String) {
        // Set explicit frame size for menu items
        let frame = NSRect(x: 0, y: 0, width: Self.menuWidth, height: Self.menuItemHeight)
        super.init(frame: frame)
        
        setupView()
        configure(title: title, time: time)
    }
    
    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupView()
    }
    
    // MARK: - Setup
    
    private func setupView() {
        wantsLayer = true
        
        // Add highlight view (behind everything)
        addSubview(highlightView)
        NSLayoutConstraint.activate([
            highlightView.topAnchor.constraint(equalTo: topAnchor),
            highlightView.leadingAnchor.constraint(equalTo: leadingAnchor),
            highlightView.bottomAnchor.constraint(equalTo: bottomAnchor),
            highlightView.trailingAnchor.constraint(equalTo: trailingAnchor)
        ])
        
        // Add time label first (right-aligned)
        addSubview(timeLabel)
        NSLayoutConstraint.activate([
            timeLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -rightMargin),
            timeLabel.centerYAnchor.constraint(equalTo: centerYAnchor)
        ])
        
        // Add title label
        addSubview(titleLabel)
        NSLayoutConstraint.activate([
            titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: leftMargin),
            titleLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            // Allow title to expand but not past the time label
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: timeLabel.leadingAnchor, constant: -horizontalSpacing)
        ])
    }
    
    // MARK: - Configuration
    
    func configure(title: String, time: String) {
        titleLabel.stringValue = title
        timeLabel.stringValue = time
    }
    
    // MARK: - Layout
    
    override var intrinsicContentSize: NSSize {
        return NSSize(width: Self.menuWidth, height: Self.menuItemHeight)
    }
    
    override func layout() {
        super.layout()
        needsDisplay = true
    }
    
    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        
        // Update highlighting based on menu item state
        let isHighlighted = enclosingMenuItem?.isHighlighted ?? false
        let isEnabled = enclosingMenuItem?.isEnabled ?? true
        
        highlightView.isHidden = !isHighlighted
        
        // Update text colors based on state
        if isHighlighted {
            titleLabel.textColor = NSColor.selectedMenuItemTextColor
            timeLabel.textColor = NSColor.selectedMenuItemTextColor
        } else if isEnabled {
            titleLabel.textColor = NSColor.controlTextColor
            timeLabel.textColor = NSColor.secondaryLabelColor
        } else {
            titleLabel.textColor = NSColor.disabledControlTextColor
            timeLabel.textColor = NSColor.disabledControlTextColor
        }
    }
    
    // MARK: - Mouse Tracking
    
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        
        // Set up tracking area for hover effects
        updateTrackingAreas()
    }
    
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        
        // Remove existing tracking areas
        for trackingArea in trackingAreas {
            removeTrackingArea(trackingArea)
        }
        
        // Add new tracking area
        let trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea)
    }
    
    // MARK: - Mouse Events
    
    override func mouseDown(with event: NSEvent) {
        // Trigger the menu item's action
        if let menuItem = enclosingMenuItem {
            _ = menuItem.target?.perform(menuItem.action, with: menuItem)
        }
    }
    
    // Disable vibrancy to ensure the visual effect view works correctly
    override var allowsVibrancy: Bool { false }
}
