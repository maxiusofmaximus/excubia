import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/state.dart';

/// One palette's tokens. Every screen reads the active one through
/// `context.colors`; `test/theme_contrast_test.dart` checks all six.
@immutable
class HerdrColors extends ThemeExtension<HerdrColors> {
  const HerdrColors({
    required this.bg,
    required this.surface,
    required this.raised,
    required this.surface3,
    required this.outlineSoft,
    required this.outlineStrong,
    required this.textPrimary,
    required this.textSecondary,
    required this.textMuted,
    required this.accent,
    required this.accentText,
    required this.accentSoft,
    required this.onAccent,
    required this.onAttention,
    required this.statusBlocked,
    required this.statusWorking,
    required this.statusDone,
    required this.statusIdle,
    required this.statusUnknown,
  });

  final Color bg;
  final Color surface;
  final Color raised;
  final Color surface3;
  final Color outlineSoft;
  final Color outlineStrong;
  final Color textPrimary;
  final Color textSecondary;
  final Color textMuted;
  final Color accent;
  final Color accentText;
  final Color accentSoft;
  final Color onAccent;
  final Color onAttention;
  final Color statusBlocked;
  final Color statusWorking;
  final Color statusDone;
  final Color statusIdle;
  final Color statusUnknown;

  /// Azul noche y latón. The approved default.
  static const HerdrColors noche = HerdrColors(
    bg: Color(0xFF0B1426),
    surface: Color(0xFF101B33),
    raised: Color(0xFF17274A),
    surface3: Color(0xFF1D2F55),
    outlineSoft: Color(0xFF1F2E50),
    outlineStrong: Color(0xFF7F93BF),
    textPrimary: Color(0xFFEFE6D2),
    textSecondary: Color(0xFFCFD8E8),
    textMuted: Color(0xFF9AA5BD),
    accent: Color(0xFFF2A93B),
    accentText: Color(0xFFF2A93B),
    accentSoft: Color(0xFF3A2E17),
    onAccent: Color(0xFF1A1204),
    onAttention: Color(0xFF0B1426),
    statusBlocked: Color(0xFFFF7A62),
    statusWorking: Color(0xFF8FB4E8),
    statusDone: Color(0xFF6FCF97),
    statusIdle: Color(0xFF9AA5BD),
    statusUnknown: Color(0xFFCFD8E8),
  );

  /// Papel cálido. The approved light palette.
  static const HerdrColors pergamino = HerdrColors(
    bg: Color(0xFFF6F0E1),
    surface: Color(0xFFFBF7EC),
    raised: Color(0xFFECE2C8),
    surface3: Color(0xFFE4D8BD),
    outlineSoft: Color(0xFFE0D4B8),
    outlineStrong: Color(0xFF7A7265),
    textPrimary: Color(0xFF1C2438),
    textSecondary: Color(0xFF34405E),
    textMuted: Color(0xFF585F71),
    accent: Color(0xFFF2A93B),
    accentText: Color(0xFF8A5208),
    accentSoft: Color(0xFFF6E4BE),
    onAccent: Color(0xFF1A1204),
    onAttention: Color(0xFFFFFFFF),
    statusBlocked: Color(0xFFA8321B),
    statusWorking: Color(0xFF2F5C9E),
    statusDone: Color(0xFF276B4E),
    statusIdle: Color(0xFF585F71),
    statusUnknown: Color(0xFF34405E),
  );

  /// Gris oscuro neutro, acento azul acero.
  static const HerdrColors carbon = HerdrColors(
    bg: Color(0xFF181818),
    surface: Color(0xFF1F1F1F),
    raised: Color(0xFF2A2A2A),
    surface3: Color(0xFF333333),
    outlineSoft: Color(0xFF303030),
    outlineStrong: Color(0xFF8A8A8A),
    textPrimary: Color(0xFFECECEC),
    textSecondary: Color(0xFFCFCFCF),
    textMuted: Color(0xFFA3A3A3),
    accent: Color(0xFF7FA6D6),
    accentText: Color(0xFF7FA6D6),
    accentSoft: Color(0xFF26313D),
    onAccent: Color(0xFF0D1622),
    onAttention: Color(0xFF181818),
    statusBlocked: Color(0xFFFF8A75),
    statusWorking: Color(0xFF8FB4E8),
    statusDone: Color(0xFF7FD6A2),
    statusIdle: Color(0xFFA3A3A3),
    statusUnknown: Color(0xFFCFCFCF),
  );

  /// Blanco roto neutro, acento azul.
  static const HerdrColors papel = HerdrColors(
    bg: Color(0xFFFCFCFC),
    surface: Color(0xFFF3F3F3),
    raised: Color(0xFFE9E9E9),
    surface3: Color(0xFFE0E0E0),
    outlineSoft: Color(0xFFDCDCDC),
    outlineStrong: Color(0xFF6B6B6B),
    textPrimary: Color(0xFF141414),
    textSecondary: Color(0xFF333333),
    textMuted: Color(0xFF5A5A5A),
    accent: Color(0xFF2B6FB5),
    accentText: Color(0xFF1F5A94),
    accentSoft: Color(0xFFDCE8F6),
    onAccent: Color(0xFFFFFFFF),
    onAttention: Color(0xFFFFFFFF),
    statusBlocked: Color(0xFFA8321B),
    statusWorking: Color(0xFF2F5C9E),
    statusDone: Color(0xFF276B4E),
    statusIdle: Color(0xFF5A5A5A),
    statusUnknown: Color(0xFF333333),
  );

  /// Azul profundo de bajo brillo.
  static const HerdrColors medianoche = HerdrColors(
    bg: Color(0xFF00183A),
    surface: Color(0xFF001F4A),
    raised: Color(0xFF0A2C5E),
    surface3: Color(0xFF12356B),
    outlineSoft: Color(0xFF16396F),
    outlineStrong: Color(0xFF7E9CCD),
    textPrimary: Color(0xFFE8F0FF),
    textSecondary: Color(0xFFC7D6EE),
    textMuted: Color(0xFF93A8CC),
    accent: Color(0xFF6FB1FF),
    accentText: Color(0xFF8FC4FF),
    accentSoft: Color(0xFF0E2A52),
    onAccent: Color(0xFF001226),
    onAttention: Color(0xFF00183A),
    statusBlocked: Color(0xFFFF8A7A),
    statusWorking: Color(0xFF8FC4FF),
    statusDone: Color(0xFF7ADFA8),
    statusIdle: Color(0xFF93A8CC),
    statusUnknown: Color(0xFFC7D6EE),
  );

  /// Negro y blanco, máxima legibilidad.
  static const HerdrColors contraste = HerdrColors(
    bg: Color(0xFF000000),
    surface: Color(0xFF0A0A0A),
    raised: Color(0xFF1A1A1A),
    surface3: Color(0xFF242424),
    outlineSoft: Color(0xFFFFFFFF),
    outlineStrong: Color(0xFFFFFFFF),
    textPrimary: Color(0xFFFFFFFF),
    textSecondary: Color(0xFFFFFFFF),
    textMuted: Color(0xFFD0D0D0),
    accent: Color(0xFFFFD400),
    accentText: Color(0xFFFFD400),
    accentSoft: Color(0xFF332B00),
    onAccent: Color(0xFF000000),
    onAttention: Color(0xFF000000),
    statusBlocked: Color(0xFFFF6B5A),
    statusWorking: Color(0xFF7FC4FF),
    statusDone: Color(0xFF6FE39B),
    statusIdle: Color(0xFFD0D0D0),
    statusUnknown: Color(0xFFFFFFFF),
  );

  /// The old names stay as aliases for the two approved palettes.
  static const HerdrColors dark = noche;
  static const HerdrColors light = pergamino;

  Color forStatus(AgentStatus status) => switch (status) {
    AgentStatus.blocked => statusBlocked,
    AgentStatus.working => statusWorking,
    AgentStatus.done => statusDone,
    AgentStatus.idle => statusIdle,
    AgentStatus.unknown => statusUnknown,
  };

  /// Colour of the status word. Blocked never takes coloured text: it is the
  /// filled chip in `StatusChip`, so the word itself stays neutral.
  Color labelForStatus(AgentStatus status) => switch (status) {
    AgentStatus.blocked => textPrimary,
    AgentStatus.idle => textMuted,
    _ => forStatus(status),
  };

  /// A distinct shape per state, so the status never rests on colour alone.
  IconData iconForStatus(AgentStatus status) => switch (status) {
    AgentStatus.blocked => Icons.warning_amber_rounded,
    AgentStatus.working => Icons.incomplete_circle,
    AgentStatus.done => Icons.check_circle,
    AgentStatus.idle => Icons.radio_button_unchecked,
    AgentStatus.unknown => Icons.help_outline,
  };

  String statusLabel(AgentStatus status) => switch (status) {
    AgentStatus.blocked => 'bloqueado',
    AgentStatus.working => 'trabajando',
    AgentStatus.done => 'terminado',
    AgentStatus.idle => 'inactivo',
    AgentStatus.unknown => 'desconocido',
  };

  @override
  HerdrColors copyWith({
    Color? bg,
    Color? surface,
    Color? raised,
    Color? surface3,
    Color? outlineSoft,
    Color? outlineStrong,
    Color? textPrimary,
    Color? textSecondary,
    Color? textMuted,
    Color? accent,
    Color? accentText,
    Color? accentSoft,
    Color? onAccent,
    Color? onAttention,
    Color? statusBlocked,
    Color? statusWorking,
    Color? statusDone,
    Color? statusIdle,
    Color? statusUnknown,
  }) => HerdrColors(
    bg: bg ?? this.bg,
    surface: surface ?? this.surface,
    raised: raised ?? this.raised,
    surface3: surface3 ?? this.surface3,
    outlineSoft: outlineSoft ?? this.outlineSoft,
    outlineStrong: outlineStrong ?? this.outlineStrong,
    textPrimary: textPrimary ?? this.textPrimary,
    textSecondary: textSecondary ?? this.textSecondary,
    textMuted: textMuted ?? this.textMuted,
    accent: accent ?? this.accent,
    accentText: accentText ?? this.accentText,
    accentSoft: accentSoft ?? this.accentSoft,
    onAccent: onAccent ?? this.onAccent,
    onAttention: onAttention ?? this.onAttention,
    statusBlocked: statusBlocked ?? this.statusBlocked,
    statusWorking: statusWorking ?? this.statusWorking,
    statusDone: statusDone ?? this.statusDone,
    statusIdle: statusIdle ?? this.statusIdle,
    statusUnknown: statusUnknown ?? this.statusUnknown,
  );

  @override
  HerdrColors lerp(ThemeExtension<HerdrColors>? other, double t) {
    if (other is! HerdrColors) return this;
    return HerdrColors(
      bg: Color.lerp(bg, other.bg, t)!,
      surface: Color.lerp(surface, other.surface, t)!,
      raised: Color.lerp(raised, other.raised, t)!,
      surface3: Color.lerp(surface3, other.surface3, t)!,
      outlineSoft: Color.lerp(outlineSoft, other.outlineSoft, t)!,
      outlineStrong: Color.lerp(outlineStrong, other.outlineStrong, t)!,
      textPrimary: Color.lerp(textPrimary, other.textPrimary, t)!,
      textSecondary: Color.lerp(textSecondary, other.textSecondary, t)!,
      textMuted: Color.lerp(textMuted, other.textMuted, t)!,
      accent: Color.lerp(accent, other.accent, t)!,
      accentText: Color.lerp(accentText, other.accentText, t)!,
      accentSoft: Color.lerp(accentSoft, other.accentSoft, t)!,
      onAccent: Color.lerp(onAccent, other.onAccent, t)!,
      onAttention: Color.lerp(onAttention, other.onAttention, t)!,
      statusBlocked: Color.lerp(statusBlocked, other.statusBlocked, t)!,
      statusWorking: Color.lerp(statusWorking, other.statusWorking, t)!,
      statusDone: Color.lerp(statusDone, other.statusDone, t)!,
      statusIdle: Color.lerp(statusIdle, other.statusIdle, t)!,
      statusUnknown: Color.lerp(statusUnknown, other.statusUnknown, t)!,
    );
  }
}
