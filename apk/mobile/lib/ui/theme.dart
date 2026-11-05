import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'palette/palette.dart';
import 'palette/palette_colors.dart';

export 'palette/palette.dart' show HerdrPalette, HerdrPalettes;
export 'palette/palette_colors.dart' show HerdrColors;

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

ThemeData buildTheme([HerdrPalette palette = HerdrPalettes.noche]) {
  final c = palette.colors;
  final brightness = palette.brightness;
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
      onTertiary: c.surface,
      error: c.statusBlocked,
      onError: c.onAttention,
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
        borderRadius: BorderRadius.circular(12),
        borderSide: BorderSide(color: c.outlineSoft),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: BorderSide(color: c.accentText),
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size.fromHeight(48),
        backgroundColor: c.accent,
        foregroundColor: c.onAccent,
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        minimumSize: const Size.fromHeight(48),
        foregroundColor: c.textPrimary,
        side: BorderSide(color: c.outlineStrong),
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(foregroundColor: c.accentText),
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
      cursorColor: c.accentText,
      selectionColor: c.accentSoft,
      selectionHandleColor: c.accentText,
    ),
    // Sin píldora: el destino activo se marca con el acento, no con una forma.
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: c.bg,
      indicatorColor: Colors.transparent,
      labelTextStyle: WidgetStateProperty.resolveWith(
        (states) => text.label.copyWith(
          color: states.contains(WidgetState.selected)
              ? c.textPrimary
              : c.textMuted,
        ),
      ),
      iconTheme: WidgetStateProperty.resolveWith(
        (states) => IconThemeData(
          color: states.contains(WidgetState.selected)
              ? c.accentText
              : c.textMuted,
        ),
      ),
    ),
    // The off track is outlineStrong, not outlineSoft: the soft line sits within
    // a hair of the page colour in Noche and Pergamino. The on track's outline is
    // accentText: the accent fill alone is under 3:1 on Pergamino. Test: theme_contrast_test.
    switchTheme: SwitchThemeData(
      trackColor: WidgetStateProperty.resolveWith(
        (states) =>
            states.contains(WidgetState.selected) ? c.accent : c.outlineStrong,
      ),
      trackOutlineColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.selected)
            ? c.accentText
            : c.outlineStrong,
      ),
      thumbColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.selected) ? c.onAccent : c.bg,
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: c.accentText),
  );
}
