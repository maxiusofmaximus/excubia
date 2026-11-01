import 'package:flutter/material.dart';

import '../theme.dart';

/// The line kinds Alfred's replies use: plain text, a `-` bullet, a `1.` item.
enum MdLine { text, bullet, numbered }

class MdLineItem {
  const MdLineItem(this.kind, this.body, {this.marker = ''});

  final MdLine kind;

  /// The line without its marker. A blank line is a `text` line with no body.
  final String body;

  /// The number of a numbered item, with its dot (`2.`).
  final String marker;
}

/// The inline styles: `**bold**`, `*italic*` and `` `code` ``.
enum MdStyle { plain, bold, italic, code }

class MdSpan {
  const MdSpan(this.text, [this.style = MdStyle.plain]);

  final String text;
  final MdStyle style;
}

final RegExp _bullet = RegExp(r'^\s*[-*]\s+(.*)$');
final RegExp _numbered = RegExp(r'^\s*(\d+)\.\s+(.*)$');

/// Splits a reply into the lines the widget draws. Nothing else of markdown is
/// read: no headings, no links, no HTML. The line breaks stay as they were.
List<MdLineItem> parseMarkdownLines(String source) => [
  for (final raw in source.split(RegExp(r'\r?\n')))
    if (_bullet.firstMatch(raw) case final match?)
      MdLineItem(MdLine.bullet, match.group(1)!)
    else if (_numbered.firstMatch(raw) case final match?)
      MdLineItem(MdLine.numbered, match.group(2)!, marker: '${match.group(1)}.')
    else
      MdLineItem(MdLine.text, raw),
];

/// Reads the inline styles of one line. A marker that is never closed, or an
/// empty pair, stays as plain text.
List<MdSpan> parseInline(String line) {
  final spans = <MdSpan>[];
  final plain = StringBuffer();
  void flush() {
    if (plain.isEmpty) return;
    spans.add(MdSpan(plain.toString()));
    plain.clear();
  }

  var index = 0;
  while (index < line.length) {
    final styled = _styledAt(line, index);
    if (styled == null) {
      plain.write(line[index]);
      index++;
      continue;
    }
    flush();
    spans.add(styled.span);
    index = styled.end;
  }
  flush();
  return spans;
}

class _Styled {
  const _Styled(this.span, this.end);

  final MdSpan span;

  /// The index just after the closing marker.
  final int end;
}

_Styled? _styledAt(String line, int index) {
  if (line.startsWith('**', index)) {
    return _closed(line, index, '**', MdStyle.bold);
  }
  if (line.startsWith('`', index)) {
    return _closed(line, index, '`', MdStyle.code);
  }
  // An italic opener must touch its text: `* item` is a bullet, not italic.
  if (line.startsWith('*', index) &&
      index + 1 < line.length &&
      line[index + 1] != ' ') {
    final close = line.indexOf('*', index + 1);
    if (close > index + 1 && line[close - 1] != ' ') {
      return _Styled(
        MdSpan(line.substring(index + 1, close), MdStyle.italic),
        close + 1,
      );
    }
  }
  return null;
}

_Styled? _closed(String line, int index, String marker, MdStyle style) {
  final start = index + marker.length;
  final close = line.indexOf(marker, start);
  if (close <= start) return null;
  return _Styled(
    MdSpan(line.substring(start, close), style),
    close + marker.length,
  );
}

/// Alfred's reply drawn with the markdown that the chat reads.
class MarkdownText extends StatelessWidget {
  const MarkdownText(this.source, {super.key, required this.style});

  final String source;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final mono = context.text.mono;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final line in parseMarkdownLines(source)) _row(line, mono),
      ],
    );
  }

  Widget _row(MdLineItem line, TextStyle mono) {
    if (line.kind == MdLine.text && line.body.trim().isEmpty) {
      return SizedBox(height: style.fontSize ?? Spacing.md);
    }
    final text = Text.rich(
      TextSpan(
        style: style,
        children: [
          for (final span in parseInline(line.body))
            TextSpan(text: span.text, style: _styleOf(span.style, mono)),
        ],
      ),
    );
    return switch (line.kind) {
      MdLine.text => text,
      MdLine.bullet => Padding(
        padding: const EdgeInsets.only(top: 2),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: 16, child: Text('•', style: style)),
            Expanded(child: text),
          ],
        ),
      ),
      MdLine.numbered => Padding(
        padding: const EdgeInsets.only(top: 2),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: 24, child: Text(line.marker, style: style)),
            Expanded(child: text),
          ],
        ),
      ),
    };
  }

  TextStyle? _styleOf(MdStyle kind, TextStyle mono) => switch (kind) {
    MdStyle.plain => null,
    MdStyle.bold => const TextStyle(fontWeight: FontWeight.w700),
    MdStyle.italic => const TextStyle(fontStyle: FontStyle.italic),
    MdStyle.code => mono,
  };
}
