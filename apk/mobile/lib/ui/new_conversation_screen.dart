import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/catalog.dart';
import 'package:herdr_notify/src/rust/api/error.dart';

import '../core/state/live_triage.dart';
import 'theme.dart';
import 'widgets/error_box.dart';
import 'widgets/surface_card.dart';

typedef CatalogLoader =
    Future<({List<CatalogEntry>? entries, ErrorInfo? error})> Function();

/// Pick a name from the host catalogue, a folder on the host and an optional
/// label; the new pane is returned to the list, which navigates when it appears.
class NewConversationScreen extends StatefulWidget {
  const NewConversationScreen({
    super.key,
    required this.triage,
    this.initialCwd,
    this.catalogLoader,
  });

  final LiveTriage triage;
  final String? initialCwd;
  final CatalogLoader? catalogLoader;

  @override
  State<NewConversationScreen> createState() => _NewConversationScreenState();
}

class _NewConversationScreenState extends State<NewConversationScreen> {
  final TextEditingController _cwd = TextEditingController();
  final TextEditingController _label = TextEditingController();

  List<CatalogEntry> _entries = const <CatalogEntry>[];
  String? _selected;
  bool _loading = true;
  bool _creating = false;
  ErrorInfo? _loadError;
  ErrorInfo? _createError;
  String? _fixing;
  ErrorInfo? _fixError;

  @override
  void initState() {
    super.initState();
    if (widget.initialCwd != null) _cwd.text = widget.initialCwd!;
    _load();
  }

  @override
  void dispose() {
    _cwd.dispose();
    _label.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _loadError = null;
    });
    final result = await (widget.catalogLoader ?? widget.triage.loadCatalog)();
    if (!mounted) return;
    final entries = result.entries ?? const <CatalogEntry>[];
    final runnable = entries.where((entry) => entry.runnable).toList();
    setState(() {
      _entries = entries;
      _loading = false;
      _loadError = result.error;
      if (_selected == null ||
          !runnable.any((entry) => entry.name == _selected)) {
        // The PC's default agent comes first when it can run; otherwise the first runnable one.
        final preferred = runnable.where((entry) => entry.isDefault);
        _selected = preferred.isNotEmpty
            ? preferred.first.name
            : (runnable.isEmpty ? null : runnable.first.name);
      }
    });
  }

  /// Repoints an entry Windows cannot start; the list reloads only on success.
  Future<void> _fix(String name) async {
    setState(() {
      _fixing = name;
      _fixError = null;
    });
    final error = await widget.triage.fixAgent(name);
    if (!mounted) return;
    setState(() {
      _fixing = null;
      _fixError = error;
    });
    if (error == null) await _load();
  }

  Future<void> _create() async {
    final agent = _selected;
    if (agent == null || _creating) return;
    setState(() {
      _creating = true;
      _createError = null;
    });
    final result = await widget.triage.createConversation(
      agent: agent,
      cwd: _cwd.text,
      label: _label.text,
    );
    if (!mounted) return;
    setState(() {
      _creating = false;
      _createError = result.error;
    });
    final created = result.created;
    if (created != null) Navigator.of(context).pop(created);
  }

  @override
  Widget build(BuildContext context) {
    final canCreate =
        !_creating && _selected != null && _cwd.text.trim().isNotEmpty;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () => Navigator.of(context).pop(),
        ),
        title: const Text('Nueva conversación'),
      ),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          Text(
            'El agente arranca en el PC donde está Excubia. La carpeta es '
            'una ruta de ese PC: el teléfono no puede explorarla.',
            style: context.text.bodySecondary,
          ),
          const SizedBox(height: Spacing.lg),
          _agentSection(),
          const SizedBox(height: Spacing.lg),
          SurfaceCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                TextField(
                  controller: _cwd,
                  style: context.text.mono,
                  onChanged: (_) => setState(() {}),
                  decoration: const InputDecoration(
                    labelText: 'Carpeta en el equipo',
                    hintText: r'D:\proyectos\mi-app',
                  ),
                ),
                const SizedBox(height: Spacing.sm),
                Text(
                  'Debe existir en el PC y ser una carpeta.',
                  style: context.text.small,
                ),
                const SizedBox(height: Spacing.md),
                TextField(
                  controller: _label,
                  maxLength: 64,
                  decoration: const InputDecoration(
                    labelText: 'Etiqueta (opcional)',
                    hintText: 'Mi conversación',
                  ),
                ),
              ],
            ),
          ),
          if (_createError != null) ...[
            const SizedBox(height: Spacing.md),
            ErrorBox(error: _createError!),
          ],
          const SizedBox(height: Spacing.lg),
          FilledButton(
            onPressed: canCreate ? _create : null,
            child: _creating
                ? Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: context.colors.onAccent,
                        ),
                      ),
                      const SizedBox(width: Spacing.sm),
                      const Text('Creando…'),
                    ],
                  )
                : const Text('Crear conversación'),
          ),
          const SizedBox(height: Spacing.lg),
        ],
      ),
    );
  }

  Widget _agentSection() {
    if (_loading) {
      return SurfaceCard(
        child: Row(
          children: [
            SizedBox(
              width: 20,
              height: 20,
              child: CircularProgressIndicator(
                strokeWidth: 2,
                color: context.colors.accentText,
              ),
            ),
            SizedBox(width: Spacing.md),
            Text('Cargando el catálogo…', style: context.text.bodySecondary),
          ],
        ),
      );
    }
    if (_loadError != null) {
      return SurfaceCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('No se pudo leer el catálogo', style: context.text.title),
            const SizedBox(height: Spacing.sm),
            ErrorBox(error: _loadError!),
            const SizedBox(height: Spacing.md),
            OutlinedButton(onPressed: _load, child: const Text('Reintentar')),
          ],
        ),
      );
    }
    if (_entries.isEmpty) {
      return SurfaceCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('No hay agentes disponibles', style: context.text.title),
            const SizedBox(height: Spacing.sm),
            Text(
              'El equipo no tiene ningún agente ejecutable en su catálogo. '
              'Instala uno (Claude, Codex, Gemini u OpenCode) y vuelve a intentarlo.',
              style: context.text.bodySecondary,
            ),
            const SizedBox(height: Spacing.md),
            OutlinedButton(onPressed: _load, child: const Text('Reintentar')),
          ],
        ),
      );
    }
    return SurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Agente', style: context.text.title),
          const SizedBox(height: Spacing.xs),
          if (!_entries.any((entry) => entry.runnable)) ...[
            Text(
              'Ningún agente de este equipo se puede usar todavía.',
              style: context.text.bodySecondary,
            ),
            const SizedBox(height: Spacing.xs),
          ],
          RadioGroup<String>(
            groupValue: _selected,
            onChanged: (value) => setState(() => _selected = value),
            child: Material(
              type: MaterialType.transparency,
              child: Column(
                children: [
                  for (final entry in _entries)
                    entry.runnable
                        ? RadioListTile<String>(
                            value: entry.name,
                            contentPadding: EdgeInsets.zero,
                            activeColor: context.colors.accentText,
                            title: Text(entry.label, style: context.text.body),
                            subtitle: entry.label == entry.name
                                ? null
                                : Text(entry.name, style: context.text.small),
                          )
                        : _unusableTile(entry),
                ],
              ),
            ),
          ),
          if (_fixError != null) ...[
            const SizedBox(height: Spacing.sm),
            ErrorBox(error: _fixError!),
          ],
        ],
      ),
    );
  }

  /// An entry the PC cannot start now: shown so the cause is visible, never selectable.
  Widget _unusableTile(CatalogEntry entry) {
    final fixing = _fixing == entry.name;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Spacing.xs),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(entry.label, style: context.text.bodySecondary),
          Text(
            entry.problem ?? 'No se puede usar en este equipo.',
            style: context.text.small,
          ),
          if (entry.fixable) ...[
            const SizedBox(height: Spacing.sm),
            OutlinedButton(
              onPressed: _fixing == null ? () => _fix(entry.name) : null,
              child: Text(fixing ? 'Arreglando…' : 'Arreglar ahora'),
            ),
          ],
        ],
      ),
    );
  }
}
