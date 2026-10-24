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
    final available = (result.entries ?? const <CatalogEntry>[])
        .where((entry) => entry.available)
        .toList();
    setState(() {
      _entries = available;
      _loading = false;
      _loadError = result.error;
      if (_selected == null ||
          !available.any((entry) => entry.name == _selected)) {
        _selected = available.isEmpty ? null : available.first.name;
      }
    });
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
            'El agente arranca en el equipo que ejecuta el host. La carpeta es '
            'una ruta de ese equipo: el teléfono no puede explorarla.',
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
                  'Debe existir en el host y ser una carpeta.',
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
                ? const Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: Colors.white,
                        ),
                      ),
                      SizedBox(width: Spacing.sm),
                      Text('Creando…'),
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
                color: context.colors.accent,
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
              'Instala uno (claude, codex, gemini u opencode) o añádelo en '
              'agents.toml y vuelve a intentarlo.',
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
          RadioGroup<String>(
            groupValue: _selected,
            onChanged: (value) => setState(() => _selected = value),
            child: Material(
              type: MaterialType.transparency,
              child: Column(
                children: [
                  for (final entry in _entries)
                    RadioListTile<String>(
                      value: entry.name,
                      contentPadding: EdgeInsets.zero,
                      activeColor: context.colors.accent,
                      title: Text(entry.label, style: context.text.body),
                      subtitle: entry.label == entry.name
                          ? null
                          : Text(entry.name, style: context.text.small),
                    ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
