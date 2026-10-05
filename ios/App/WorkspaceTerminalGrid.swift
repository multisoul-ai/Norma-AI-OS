import SwiftUI
import CoreText
import NormaCore

/// Fixed Mac columns inside a two-axis native scroll view. No input or terminal resize API.
struct WorkspaceTerminalGrid: UIViewRepresentable {
    let frame: WorkspaceTerminalFrame
    let fontSize: CGFloat
    @Binding var followLatest: Bool
    let returnGeneration: Int
    let reading: TerminalGridReadingPosition
    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIView(context: Context) -> TerminalGridScrollView {
        let view = TerminalGridScrollView()
        view.reading = reading
        view.delegate = context.coordinator
        return view
    }
    func updateUIView(_ view: TerminalGridScrollView, context: Context) {
        context.coordinator.parent = self
        view.apply(frame, fontSize: fontSize, following: followLatest, returnGeneration: returnGeneration)
    }
    final class Coordinator: NSObject, UIScrollViewDelegate {
        var parent: WorkspaceTerminalGrid
        init(_ parent: WorkspaceTerminalGrid) { self.parent = parent }
        func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
            if scrollView.panGestureRecognizer.velocity(in: scrollView).y > 0 { parent.followLatest = false }
        }
        func scrollViewDidScroll(_ scrollView: UIScrollView) {
            (scrollView as? TerminalGridScrollView)?.saveReadingPosition()
            // Also recognize slow upward drags after the initial velocity was zero.
            if scrollView.isDragging && scrollView.panGestureRecognizer.translation(in: scrollView).y > 4 {
                parent.followLatest = false
            }
        }
    }
}

/// Sheet-local reading position survives removing the terminal tab from the view hierarchy.
final class TerminalGridReadingPosition { var offset = CGPoint.zero; var startLine: Int? }

final class TerminalGridScrollView: UIScrollView {
    let grid = TerminalGridCanvas()
    var reading: TerminalGridReadingPosition?
    private var following = true
    private var returnGeneration = 0
    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(grid)
        alwaysBounceHorizontal = true; alwaysBounceVertical = true
        isDirectionalLockEnabled = false; contentInsetAdjustmentBehavior = .never
        accessibilityLabel = "只读终端，可上下阅读和左右滚动"
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    func apply(_ frame: WorkspaceTerminalFrame, fontSize: CGFloat, following: Bool, returnGeneration: Int) {
        let previous = grid.terminal
        let restoredOffset = reading?.startLine == frame.startLine ? reading?.offset : nil
        let oldHeight = grid.cellSize.height
        let readingLine = oldHeight > 0 ? contentOffset.y / oldHeight : 0
        let readingColumn = grid.cellSize.width > 0 ? contentOffset.x / grid.cellSize.width : 0
        let resized = grid.fontSize != fontSize
        grid.apply(frame, fontSize: fontSize)
        backgroundColor = UIColor(terminalRGB: frame.background)
        self.following = following
        let returned = self.returnGeneration != returnGeneration
        self.returnGeneration = returnGeneration
        resizeContent()
        if following || returned { scrollToLatest() }
        else if previous == nil, let restoredOffset { contentOffset = restoredOffset }
        else if previous?.startLine != frame.startLine {
            // Page navigation opens the latest end of the older page; a paused capture stays still.
            contentOffset.y = max(0, contentSize.height - bounds.height)
        } else if resized {
            contentOffset = CGPoint(x: readingColumn * grid.cellSize.width, y: readingLine * grid.cellSize.height)
        }
        saveReadingPosition()
    }
    override func layoutSubviews() {
        super.layoutSubviews()
        resizeContent()
        if following && !isDragging && !isDecelerating { scrollToLatest() }
    }
    private func resizeContent() {
        guard let terminal = grid.terminal else { return }
        let size = CGSize(width: max(bounds.width, CGFloat(terminal.cols) * grid.cellSize.width + 16),
                          height: max(bounds.height, CGFloat(terminal.rows.count) * grid.cellSize.height + 16))
        if grid.frame.size != size { grid.frame = CGRect(origin: .zero, size: size); contentSize = size }
    }
    private func scrollToLatest() { contentOffset.y = max(0, contentSize.height - bounds.height) }
    func saveReadingPosition() { reading?.offset = contentOffset; reading?.startLine = grid.terminal?.startLine }
}

final class TerminalGridCanvas: UIView {
    private(set) var terminal: WorkspaceTerminalFrame?
    private(set) var fontSize: CGFloat = 0
    private(set) var cellSize = CGSize.zero
    private var fonts: [UInt16: UIFont] = [:]
    // Exposed internally to verify that progress, color and cursor changes invalidate their rows.
    private(set) var changedRows: Set<Int> = []
    override init(frame: CGRect) {
        super.init(frame: frame); isOpaque = true; contentMode = .redraw
        isAccessibilityElement = true; accessibilityLabel = "终端画面"; accessibilityTraits = .staticText
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    func apply(_ next: WorkspaceTerminalFrame, fontSize: CGFloat) {
        let previous = terminal
        let relayout = self.fontSize != fontSize || previous?.cols != next.cols || previous?.startLine != next.startLine || previous?.background != next.background
        if self.fontSize != fontSize {
            self.fontSize = fontSize; fonts.removeAll()
            let font = UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
            cellSize = CGSize(width: ("M" as NSString).size(withAttributes: [.font:font]).width,
                              height: ceil(font.lineHeight + 2))
        }
        terminal = next; backgroundColor = UIColor(terminalRGB: next.background)
        accessibilityValue = next.text()
        changedRows = []
        for index in next.rows.indices where previous?.rows.indices.contains(index) != true || previous?.rows[index] != next.rows[index] { changedRows.insert(index) }
        for cursor in [previous?.cursor, next.cursor].compactMap({ $0 }) {
            if previous?.cursor != next.cursor { changedRows.insert(cursor.line - next.startLine) }
        }
        if relayout || previous?.rows.count != next.rows.count { setNeedsDisplay() }
        else { for row in changedRows where row >= 0 { setNeedsDisplay(CGRect(x: 0, y: 8 + CGFloat(row) * cellSize.height, width: bounds.width, height: cellSize.height)) } }
    }
    override func draw(_ rect: CGRect) {
        guard let terminal, let context = UIGraphicsGetCurrentContext(), cellSize.height > 0 else { return }
        context.setFillColor(UIColor(terminalRGB: terminal.background).cgColor); context.fill(rect)
        let first = max(0, Int(floor((rect.minY - 8) / cellSize.height)))
        let end = min(terminal.rows.count, Int(ceil((rect.maxY - 8) / cellSize.height)))
        guard first < end else { return }
        for rowIndex in first..<end {
            let row = terminal.rows[rowIndex]
            for (column, cell) in row.enumerated() {
                let box = CGRect(x: 8 + CGFloat(column) * cellSize.width, y: 8 + CGFloat(rowIndex) * cellSize.height, width: cellSize.width, height: cellSize.height)
                guard box.intersects(rect) else { continue }
                context.setFillColor(UIColor(terminalRGB: cell.bg).cgColor); context.fill(box)
            }
            for (column, cell) in row.enumerated() where !cell.spacer && !cell.hidden {
                var width = cellSize.width
                if column + 1 < row.count && row[column + 1].spacer { width *= 2 }
                let box = CGRect(x: 8 + CGFloat(column) * cellSize.width, y: 8 + CGFloat(rowIndex) * cellSize.height, width: width, height: cellSize.height)
                guard box.intersects(rect) else { continue }
                drawCell(cell, in: box, context: context)
            }
        }
        if let cursor = terminal.cursor, cursor.line >= terminal.startLine && cursor.line < terminal.startLine + terminal.rows.count {
            let box = CGRect(x: 8 + CGFloat(cursor.column) * cellSize.width, y: 8 + CGFloat(cursor.line - terminal.startLine) * cellSize.height, width: cellSize.width, height: cellSize.height)
            context.setStrokeColor(UIColor(terminalRGB: terminal.foreground).cgColor)
            context.setLineWidth(1); context.stroke(box.insetBy(dx: 0.5, dy: 0.5))
        }
    }
    private func drawCell(_ cell: WorkspaceTerminalCell, in box: CGRect, context: CGContext) {
        let traits = cell.flags & 3
        let font: UIFont
        if let cached = fonts[traits] { font = cached }
        else {
            let base = UIFont.monospacedSystemFont(ofSize: fontSize, weight: traits & 1 != 0 ? .bold : .regular)
            font = traits & 2 != 0 ? UIFont(descriptor: base.fontDescriptor.withSymbolicTraits(.traitItalic) ?? base.fontDescriptor, size: fontSize) : base
            fonts[traits] = font
        }
        let color = UIColor(terminalRGB: cell.fg)
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: cell.text, attributes: [.font:font, .foregroundColor:color]))
        context.saveGState(); context.clip(to: box)
        context.textMatrix = .identity
        context.translateBy(x: box.minX, y: box.minY + font.ascender + 1); context.scaleBy(x: 1, y: -1)
        context.textPosition = .zero; CTLineDraw(line, context); context.restoreGState()
        context.setStrokeColor(color.cgColor); context.setLineWidth(1)
        if cell.flags & 4 != 0 { context.move(to: CGPoint(x: box.minX, y: box.maxY - 2)); context.addLine(to: CGPoint(x: box.maxX, y: box.maxY - 2)); context.strokePath() }
        if cell.flags & 8 != 0 { context.move(to: CGPoint(x: box.minX, y: box.midY)); context.addLine(to: CGPoint(x: box.maxX, y: box.midY)); context.strokePath() }
    }
}

private extension UIColor {
    convenience init(terminalRGB value: UInt32) {
        self.init(red: CGFloat((value >> 16) & 255) / 255, green: CGFloat((value >> 8) & 255) / 255, blue: CGFloat(value & 255) / 255, alpha: 1)
    }
}
