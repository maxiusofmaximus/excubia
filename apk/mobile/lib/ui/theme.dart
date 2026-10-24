import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

/// The approved "warm studio" palette from the wf40–wf46 wireframes. Every
/// screen reads it through `context.colors`; the light and dark values are the
/// two columns of `ui-analysis/index.html` §23.
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
    required this.accentSoft,
    required this.onAccent,
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
  final Color accentSoft;
  final Color onAccent;
  final Color statusBlocked;
  final Color statusWorking;
  final Color statusDone;
  final Color statusIdle;
  final Color statusUnknown;

  static const HerdrColors dark = HerdrColors(
    bg: Color(0xFF1B1612),
    surface: Color(0xFF251E18),
    raised: Color(0xFF2A221B),
    surface3: Color(0xFF342A21),
    outlineSoft: Color(0xFF41362B),
    outlineStrong: Color(0xFF5A4B3B),
    textPrimary: Color(0xFFF3EBDF),
    textSecondary: Color(0xFFCDBFAE),
    textMuted: Color(0xFFA39583),
    accent: Color(0xFF7566B0),
    accentSoft: Color(0xFF2D2644),
    onAccent: Color(0xFFFFFFFF),
    statusBlocked: Color(0xFFD9795C),
    statusWorking: Color(0xFFB9AAE6),
    statusDone: Color(0xFF8FC48F),
    statusIdle: Color(0xFFA39583),
    statusUnknown: Color(0xFFCDBFAE),
  );

  static const HerdrColors light = HerdrColors(
    bg: Color(0xFFF6F0E6),
    surface: Color(0xFFFFFAF2),
    raised: Color(0xFFEEE5D6),
    surface3: Color(0xFFE3D8C6),
    outlineSoft: Color(0xFFD8CBB6),
    outlineStrong: Color(0xFFB7A58D),
    textPrimary: Color(0xFF2B2118),
    textSecondary: Color(0xFF5A4C3E),
    textMuted: Color(0xFF6F6254),
    accent: Color(0xFF6B5A9E),
    accentSoft: Color(0xFFE9E2F3),
    onAccent: Color(0xFFFFFFFF),
    statusBlocked: Color(0xFFA8452B),
    statusWorking: Color(0xFF5A4A8C),
    statusDone: Color(0xFF3B7343),
    statusIdle: Color(0xFF6F6254),
    statusUnknown: Color(0xFF5A4C3E),
  );

  Color forStatus(AgentStatus status) => switch (status) {
    AgentStatus.blocked => statusBlocked,
    AgentStatus.working => statusWorking,
    AgentStatus.done => statusDone,
    AgentStatus.idle => statusIdle,
    AgentStatus.unknown => statusUnknown,
  };

  Color labelForStatus(AgentStatus status) =>
      status == AgentStatus.idle ? textMuted : forStatus(status);

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
    Color? accentSoft,
    Color? onAccent,
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
    accentSoft: accentSoft ?? this.accentSoft,
    onAccent: onAccent ?? this.onAccent,
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
      accentSoft: Color.lerp(accentSoft, other.accentSoft, t)!,
      onAccent: Color.lerp(onAccent, other.onAccent, t)!,
      statusBlocked: Color.lerp(statusBlocked, other.statusBlocked, t)!,
      statusWorking: Color.lerp(statusWorking, other.statusWorking, t)!,
      statusDone: Color.lerp(statusDone, other.statusDone, t)!,
      statusIdle: Color.lerp(statusIdle, other.statusIdle, t)!,
      statusUnknown: Color.lerp(statusUnknown, other.statusUnknown, t)!,
    );
  }
}

extension HerdrTheme on BuildContext {
  HerdrColors get colors =>
      Theme.of(this).extension<HerdrColors>() ?? HerdrColors.dark;
  AppText get text => AppText(colors);
}

/// Spacing on a 4dp grid.
class Spacing {
  Spacing._();

  static const double xs = 4;
  static const double sm = 8;
  static const double md = 12;
  static const double lg = 16;
  static const double xl = 24;
  static const double xxl = 32;
}

/// Type ramp bound to the active palette, so a light theme cannot inherit a
/// dark text colour by accident.
class AppText {
  const AppText(this.c);

  final HerdrColors c;

  TextStyle get headline => TextStyle(
    fontWeight: FontWeight.w600,
    fontSize: 24,
    height: 30 / 24,
    letterSpacing: -0.3,
    color: c.textPrimary,
  );

  TextStyle get title => TextStyle(
    fontWeight: FontWeight.w600,
    fontSize: 16,
    height: 21 / 16,
    letterSpacing: -0.1,
    color: c.textPrimary,
  );

  TextStyle get body =>
      TextStyle(fontSize: 14, height: 20 / 14, color: c.textPrimary);

  TextStyle get bodySecondary =>
      TextStyle(fontSize: 14, height: 20 / 14, color: c.textSecondary);

  TextStyle get small =>
      TextStyle(fontSize: 12, height: 17 / 12, color: c.textSecondary);

  TextStyle get label => TextStyle(
    fontWeight: FontWeight.w500,
    fontSize: 12,
    height: 16 / 12,
    letterSpacing: 0.2,
    color: c.textSecondary,
  );

  TextStyle get mono => TextStyle(
    fontFamily: 'monospace',
    fontSize: 13,
    height: 20 / 13,
    color: c.textPrimary,
  );
}

ThemeData buildTheme([Brightness brightness = Brightness.dark]) {
  final c = brightness == Brightness.dark
      ? HerdrColors.dark
      : HerdrColors.light;
  final text = AppText(c);
  final base = ThemeData(
    brightness: brightness,
    useMaterial3: true,
    scaffoldBackgroundColor: c.bg,
    extensions: <ThemeExtension<dynamic>>[c],
    colorScheme: ColorScheme(
      brightness: brightness,
      primary: c.accent,
      onPrimary: c.onAccent,
      primaryContainer: c.accentSoft,
      onPrimaryContainer: c.textPrimary,
      secondary: c.accent,
      onSecondary: c.onAccent,
      secondaryContainer: c.raised,
      onSecondaryContainer: c.textPrimary,
      tertiary: c.statusWorking,
      onTertiary: c.onAccent,
      error: c.statusBlocked,
      onError: c.onAccent,
      surface: c.surface,
      onSurface: c.textPrimary,
      surfaceContainerHighest: c.surface3,
      onSurfaceVariant: c.textSecondary,
      outline: c.outlineSoft,
      outlineVariant: c.outlineStrong,
      shadow: Colors.black,
      scrim: Colors.black,
      inverseSurface: c.textPrimary,
      onInverseSurface: c.bg,
      inversePrimary: c.accentSoft,
    ),
    textTheme: TextTheme(
      bodyLarge: text.body,
      bodyMedium: text.body,
      bodySmall: text.small,
      titleMedium: text.title,
      titleLarge: text.title,
      labelLarge: text.label,
      labelMedium: text.label,
      labelSmall: text.label,
      headlineLarge: text.headline,
    ).apply(bodyColor: c.textPrimary, displayColor: c.textPrimary),
  );
  return base.copyWith(
    appBarTheme: AppBarTheme(
      backgroundColor: c.bg,
      foregroundColor: c.textPrimary,
      elevation: 0,
      centerTitle: false,
      systemOverlayStyle: SystemUiOverlayStyle(
        statusBarColor: Colors.transparent,
        statusBarIconBrightness: brightness == Brightness.dark
            ? Brightness.light
            : Brightness.dark,
        statusBarBrightness: brightness,
        systemNavigationBarColor: c.bg,
        systemNavigationBarIconBrightness: brightness == Brightness.dark
            ? Brightness.light
            : Brightness.dark,
      ),
    ),
    dividerColor: c.outlineSoft,
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: c.raised,
      hintStyle: TextStyle(color: c.textMuted),
      labelStyle: TextStyle(color: c.textSecondary),
      contentPadding: const EdgeInsets.symmetric(
        horizontal: Spacing.md,
        vertical: Spacing.md,
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: c.outlineSoft),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: c.accent),
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size.fromHeight(48),
        backgroundColor: c.accent,
        foregroundColor: c.onAccent,
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        minimumSize: const Size.fromHeight(48),
        foregroundColor: c.textPrimary,
        side: BorderSide(color: c.outlineStrong),
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
      ),
    ),
    snackBarTheme: SnackBarThemeData(
      backgroundColor: c.raised,
      contentTextStyle: TextStyle(color: c.textPrimary),
      behavior: SnackBarBehavior.floating,
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: c.raised,
      titleTextStyle: text.title,
      contentTextStyle: text.bodySecondary,
    ),
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: c.accent,
      selectionColor: c.accentSoft,
      selectionHandleColor: c.accent,
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: c.raised,
      indicatorColor: c.accentSoft,
      labelTextStyle: WidgetStatePropertyAll(text.label),
      iconTheme: WidgetStatePropertyAll(IconThemeData(color: c.textSecondary)),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: c.accent),
  );
}
