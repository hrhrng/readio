import SwiftUI

struct ReadioSentenceTextProjection: Equatable {
    let before: String
    let highlighted: String
    let after: String
}

struct ReadioStyledTextSegment: Equatable {
    let text: String
    let bold: Bool
    let italic: Bool
    let highlighted: Bool
}

/// Renders a sentence while treating Rust's Unicode-scalar range as the only
/// authority for the active word. SwiftUI owns styling; it never tokenizes or
/// derives a word index from playback progress.
struct SentenceTextView: View {
    @Environment(\.readioTheme) private var theme
    let sentence: Sentence
    let isActive: Bool
    let wordRange: ReadioScalarRange?
    let runs: [StyledRun]?
    let onTap: () -> Void

    init(
        sentence: Sentence,
        isActive: Bool,
        wordRange: ReadioScalarRange?,
        runs: [StyledRun]? = nil,
        onTap: @escaping () -> Void
    ) {
        self.sentence = sentence
        self.isActive = isActive
        self.wordRange = wordRange
        self.runs = runs
        self.onTap = onTap
    }

    var body: some View {
        Button(action: onTap) {
            styledTextView(highlightRange: isActive ? wordRange : nil)
        }
        .buttonStyle(.plain)
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isActive ? theme.selected : Color.clear)
        )
        .animation(.easeInOut(duration: 0.15), value: isActive)
        .id(sentence.index)
    }

    private func styledTextView(highlightRange: ReadioScalarRange?) -> some View {
        let segments = Self.projectStyledText(
            sentence.text,
            runs: runs ?? [StyledRun(text: sentence.text)],
            highlightRange: highlightRange
        )
        var content = AttributedString()
        for segment in segments {
            var value = AttributedString(segment.text)
            var font = Font.body
            if segment.bold || segment.highlighted {
                font = font.weight(.semibold)
            }
            if segment.italic {
                font = font.italic()
            }
            value.font = font
            if segment.highlighted {
                value.foregroundColor = theme.accentStrong
            }
            content.append(value)
        }
        return Text(content)
            .font(.body)
            .foregroundStyle(theme.ink)
            .multilineTextAlignment(.leading)
    }

    static func projectStyledText(
        _ text: String,
        runs: [StyledRun],
        highlightRange: ReadioScalarRange?
    ) -> [ReadioStyledTextSegment] {
        let scalars = Array(text.unicodeScalars)
        let normalizedRuns = runs.map { run in
            (
                run: run,
                length: run.text.unicodeScalars.count
            )
        }
        guard normalizedRuns.reduce(0, { $0 + $1.length }) == scalars.count else {
            return [
                ReadioStyledTextSegment(
                    text: text,
                    bold: false,
                    italic: false,
                    highlighted: false
                )
            ]
        }

        var runRanges: [(start: Int, end: Int, run: StyledRun)] = []
        var cursor = 0
        var boundaries: Set<Int> = [0, scalars.count]
        for value in normalizedRuns {
            let end = cursor + value.length
            runRanges.append((cursor, end, value.run))
            boundaries.insert(cursor)
            boundaries.insert(end)
            cursor = end
        }
        let highlight: (start: Int, end: Int)? = highlightRange.flatMap { range in
            guard let start = Int(exactly: range.start),
                  let end = Int(exactly: range.end),
                  0 <= start,
                  start < end,
                  end <= scalars.count else { return nil }
            boundaries.insert(start)
            boundaries.insert(end)
            return (start, end)
        }

        let ordered = boundaries.sorted()
        var segments: [ReadioStyledTextSegment] = []
        for (start, end) in zip(ordered, ordered.dropFirst()) where start < end {
            let style = runRanges.first {
                $0.start <= start && end <= $0.end
            }?.run
            let segment = ReadioStyledTextSegment(
                text: String(String.UnicodeScalarView(scalars[start..<end])),
                bold: style?.bold ?? false,
                italic: style?.italic ?? false,
                highlighted: highlight.map {
                    $0.start <= start && end <= $0.end
                } ?? false
            )
            if let previous = segments.last,
               previous.bold == segment.bold,
               previous.italic == segment.italic,
               previous.highlighted == segment.highlighted {
                segments[segments.count - 1] = ReadioStyledTextSegment(
                    text: previous.text + segment.text,
                    bold: segment.bold,
                    italic: segment.italic,
                    highlighted: segment.highlighted
                )
            } else {
                segments.append(segment)
            }
        }
        return segments
    }

    static func projectScalarHighlight(
        _ text: String,
        range: ReadioScalarRange
    ) -> ReadioSentenceTextProjection {
        let scalars = Array(text.unicodeScalars)
        guard range.start < range.end,
              let start = Int(exactly: range.start),
              let end = Int(exactly: range.end),
              start >= 0,
              end <= scalars.count else {
            return ReadioSentenceTextProjection(
                before: text,
                highlighted: "",
                after: ""
            )
        }
        return ReadioSentenceTextProjection(
            before: String(String.UnicodeScalarView(scalars[0..<start])),
            highlighted: String(String.UnicodeScalarView(scalars[start..<end])),
            after: String(String.UnicodeScalarView(scalars[end..<scalars.count]))
        )
    }
}

#Preview("Active Sentence") {
    VStack(spacing: 12) {
        SentenceTextView(
            sentence: Sentence(index: 0, text: "The quick brown fox jumps."),
            isActive: true,
            wordRange: ReadioScalarRange(start: 4, end: 9),
            onTap: {}
        )

        SentenceTextView(
            sentence: Sentence(index: 1, text: "This sentence is not active."),
            isActive: false,
            wordRange: nil,
            onTap: {}
        )
    }
    .padding()
}
