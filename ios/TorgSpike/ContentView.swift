import SwiftUI

// Sample documents. The same commands (`outline`, `cycleTodo`) drive both formats — that's the
// whole point of the shared `StructureProvider` in torg-core.
private let sampleOrg = """
* Project torg
** TODO [#A] Ship the iPad spike :ios:
Prove the Rust core drives a SwiftUI view.
** DONE Wire up UniFFI
* Personal
** Groceries
"""

private let sampleMarkdown = """
# Project torg
## TODO Ship the iPad spike
Prove the Rust core drives a SwiftUI view.
## DONE Wire up UniFFI
# Personal
## Groceries
"""

struct ContentView: View {
    @State private var markdown = false
    @State private var text = sampleOrg
    @State private var collapsed: Set<UInt32> = []   // heading lines that are folded

    // --- everything below is computed by torg-core over the FFI ---
    private var headings: [HeadingInfo] { outline(text: text, markdown: markdown) }
    private var headingByLine: [UInt32: HeadingInfo] {
        Dictionary(headings.map { ($0.line, $0) }, uniquingKeysWith: { a, _ in a })
    }
    private var lines: [String] { text.components(separatedBy: "\n") }

    /// A line is hidden if it falls inside some collapsed heading's subtree.
    private func hidden(_ line: UInt32) -> Bool {
        headings.contains { collapsed.contains($0.line) && line > $0.line && line <= $0.lastLine }
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(Array(lines.enumerated()), id: \.offset) { idx, raw in
                    let line = UInt32(idx)
                    if !hidden(line) {
                        row(line: line, raw: raw)
                    }
                }
            }
            .listStyle(.plain)
            .navigationTitle("torg spike")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Picker("Format", selection: $markdown) {
                        Text("Org").tag(false)
                        Text("Markdown").tag(true)
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: markdown) { _ in
                        text = markdown ? sampleMarkdown : sampleOrg
                        collapsed = []
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func row(line: UInt32, raw: String) -> some View {
        if let h = headingByLine[line] {
            HStack(spacing: 8) {
                Image(systemName: collapsed.contains(line) ? "chevron.right" : "chevron.down")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: 14)
                if let todo = h.todo {
                    Text(todo)
                        .font(.caption2).bold()
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(todo == "DONE" ? Color.green.opacity(0.25) : Color.orange.opacity(0.25))
                        .clipShape(Capsule())
                }
                Text(h.title).fontWeight(.semibold)
                Spacer()
            }
            .padding(.leading, CGFloat(h.level - 1) * 18)
            .contentShape(Rectangle())
            .onTapGesture { toggleFold(line) }
            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                Button {
                    // The one real edit in the spike — mutated by torg-core, round-tripped as text.
                    text = cycleTodo(text: text, line: line, markdown: markdown)
                } label: {
                    Label("Cycle TODO", systemImage: "checkmark.circle")
                }
                .tint(.blue)
            }
        } else {
            Text(raw.isEmpty ? " " : raw)
                .font(.body.monospaced())
                .foregroundStyle(.secondary)
                .padding(.leading, 26)
        }
    }

    private func toggleFold(_ line: UInt32) {
        if collapsed.contains(line) {
            collapsed.remove(line)
        } else {
            collapsed.insert(line)
        }
    }
}

// The #Preview macro needs Xcode 15 / Swift 5.9; use PreviewProvider for Xcode 14.2.
struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
    }
}
