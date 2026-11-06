import 'package:flutter/material.dart';

import '../../core/api/read_grant.dart';
import '../../core/api/restore_api.dart';
import '../../core/api/restore_models.dart';
import '../theme.dart';
import '../widgets/empty_state.dart';
import 'restore_text.dart';
import 'restore_widgets.dart';

part 'restore_steps.dart';

/// «Copias del turno» of one agent. Each step is one view of this screen: the copies,
/// what one would change, the confirmation, and the result with its undo. The copies
/// are kept only while the screen is open, because the agent id is only a pane id.
class RestoreScreen extends StatefulWidget {
  const RestoreScreen({
    super.key,
    required this.api,
    required this.agent,
    required this.title,
  });

  final RestoreApi api;

  /// The pane id of the agent.
  final String agent;

  /// The agent's name, shown above its copies.
  final String title;

  @override
  State<RestoreScreen> createState() => _RestoreScreenState();
}

enum _Stage { list, detail, confirm, done, undone }

class _RestoreScreenState extends State<RestoreScreen> {
  _Stage _stage = _Stage.list;

  // The list: null while it loads, or the failure that stopped it.
  List<RestorePoint>? _points;
  Object? _listFailure;

  // The copy the person opened, and what it would change.
  RestorePoint? _point;
  RestoreChanges? _changes;
  Object? _detailFailure;

  // The restore and its undo. A refused call stays on the step that made it.
  RestoreResult? _done;
  Object? _slotFailure;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _loadList();
  }

  Future<void> _loadList() async {
    setState(() {
      _stage = _Stage.list;
      _points = null;
      _listFailure = null;
    });
    try {
      final points = await widget.api.points(widget.agent);
      if (!mounted) return;
      setState(() => _points = points);
    } catch (error) {
      if (!mounted) return;
      setState(() => _listFailure = error);
    }
  }

  Future<void> _openPoint(RestorePoint point) async {
    setState(() {
      _stage = _Stage.detail;
      _point = point;
      _changes = null;
      _detailFailure = null;
    });
    try {
      final changes = await widget.api.changes(point.id);
      if (!mounted || _point != point) return;
      setState(() => _changes = changes);
    } catch (error) {
      if (!mounted || _point != point) return;
      setState(() => _detailFailure = error);
    }
  }

  void _showDetail() => setState(() => _stage = _Stage.detail);

  void _confirm() => setState(() {
    _stage = _Stage.confirm;
    _slotFailure = null;
  });

  /// Runs one restore or undo. Its failure is kept for the step, so it can be read again.
  Future<RestoreResult?> _run(String pointId) async {
    setState(() {
      _busy = true;
      _slotFailure = null;
    });
    try {
      final result = await widget.api.restore(pointId);
      if (mounted) setState(() => _busy = false);
      return result;
    } catch (error) {
      if (mounted) {
        setState(() {
          _busy = false;
          _slotFailure = error;
        });
      }
      return null;
    }
  }

  Future<void> _restore() async {
    final point = _point;
    if (point == null || _busy) return;
    final result = await _run(point.id);
    if (result == null || !mounted) return;
    setState(() {
      _done = result;
      _stage = _Stage.done;
    });
  }

  /// The undo is the restore of the copy the first restore kept.
  Future<void> _undo() async {
    final undo = _done?.undoPoint;
    if (undo == null || _busy) return;
    final result = await _run(undo);
    if (result == null || !mounted) return;
    setState(() => _stage = _Stage.undone);
  }

  void _back() {
    switch (_stage) {
      case _Stage.detail:
        _loadList();
      case _Stage.confirm:
        _showDetail();
      case _Stage.list || _Stage.done || _Stage.undone:
        Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    backgroundColor: context.colors.bg,
    appBar: AppBar(
      leading: IconButton(
        icon: const Icon(Icons.arrow_back),
        tooltip: 'Volver',
        onPressed: _back,
      ),
      title: const Text('Copias del turno'),
    ),
    body: switch (_stage) {
      _Stage.list => _listBody(),
      _Stage.detail => _detailBody(),
      _Stage.confirm => _confirmBody(),
      _Stage.done => _doneBody(),
      _Stage.undone => _undoneBody(),
    },
  );
}
