import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/chat/markdown_text.dart';

List<(MdLine, String, String)> _lines(String source) => [
  for (final line in parseMarkdownLines(source))
    (line.kind, line.body, line.marker),
];

List<(String, MdStyle)> _spans(String line) => [
  for (final span in parseInline(line)) (span.text, span.style),
];

void main() {
  test('reads bullets, numbers and plain lines, keeping the breaks', () {
    expect(_lines('uno\n- dos\n* tres\n2. cuatro\n'), [
      (MdLine.text, 'uno', ''),
      (MdLine.bullet, 'dos', ''),
      (MdLine.bullet, 'tres', ''),
      (MdLine.numbered, 'cuatro', '2.'),
      (MdLine.text, '', ''),
    ]);
  });

  test('reads bold, italic and code, and leaves the rest as text', () {
    expect(_spans('**negrita** y *cursiva* y `código`'), [
      ('negrita', MdStyle.bold),
      (' y ', MdStyle.plain),
      ('cursiva', MdStyle.italic),
      (' y ', MdStyle.plain),
      ('código', MdStyle.code),
    ]);
    expect(_spans('a * b'), [('a * b', MdStyle.plain)]);
    expect(_spans('precio **'), [('precio **', MdStyle.plain)]);
  });
}
