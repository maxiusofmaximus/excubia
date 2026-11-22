import 'package:flutter/material.dart';

import '../../core/state/live_triage.dart' show DocTool;
import '../../src/rust/api/error.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';

/// What `GET /v1/doc-tools` returned: the tools, the error to render, or
/// `unsupported` when the host predates the route.
typedef DocToolsLoad = ({
  List<DocTool>? tools,
  ErrorInfo? error,
  bool unsupported,
});

typedef DocToolsLoader = Future<DocToolsLoad> Function();

/// Lists the PC's document tools under the installed agents: each one either
/// installed or missing. A host without the route hides the section.
class DocToolsSection extends StatefulWidget {
  const DocToolsSection({super.key, required this.load});

  final DocToolsLoader load;

  @override
  State<DocToolsSection> createState() => _DocToolsSectionState();
}

class _DocToolsSectionState extends State<DocToolsSection> {
  List<DocTool>? _tools;
  ErrorInfo? _failure;
  bool _unsupported = false;

  @override
  void initState() {
    super.initState();
    _read();
  }

  Future<void> _read() async {
    final result = await widget.load();
    if (!mounted) return;
    setState(() {
      _tools = result.tools;
      _failure = result.error;
      _unsupported = result.unsupported;
    });
  }

  @override
  Widget build(BuildContext context) {
    if (_unsupported) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // The section owns the gap so a host without the route leaves none.
        const SizedBox(height: Spacing.xl),
        Text('HERRAMIENTAS DE DOCUMENTOS', style: context.text.label),
        const SizedBox(height: Spacing.sm),
        SurfaceCard(child: _body(context)),
      ],
    );
  }

  Widget _body(BuildContext context) {
    final failure = _failure;
    if (failure != null) {
      return Text(failure.message, style: context.text.bodySecondary);
    }
    final tools = _tools;
    if (tools == null) return Text('Leyendo…', style: context.text.body);
    return _column([
      for (final tool in tools) _toolRow(context, tool),
      Text(
        'Sirven para abrir y crear Excel, Word y PDF en el chat.',
        style: context.text.bodySecondary,
      ),
    ]);
  }

  Widget _toolRow(BuildContext context, DocTool tool) => Row(
    children: [
      Expanded(child: Text(tool.name, style: context.text.body)),
      Text(
        tool.available ? 'Instalada' : 'Falta en este PC',
        style: context.text.bodySecondary,
      ),
    ],
  );

  static Widget _column(List<Widget> children) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      for (final child in children) ...[
        child,
        const SizedBox(height: Spacing.xs),
      ],
    ],
  );
}
