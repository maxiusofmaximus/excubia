part of 'restore_screen.dart';

const EdgeInsets _pagePadding = EdgeInsets.fromLTRB(
  Spacing.lg,
  Spacing.lg,
  Spacing.lg,
  Spacing.xl,
);

/// The views of the steps. Each one is built from the state alone, so a step that
/// fails or waits shows the same words wherever it is.
extension _RestoreSteps on _RestoreScreenState {
  Widget _loadingBody() => Semantics(
    label: 'Cargando copias',
    child: Center(child: CircularProgressIndicator()),
  );

  /// A read the host refused: a device still waiting for its grant waits, anything else can be read again.
  Widget _readFailure({
    required Object failure,
    required String title,
    required VoidCallback retry,
  }) {
    final waiting = restoreWaitingText(failure);
    if (waiting != null) {
      return ListView(
        children: [
          EmptyState(
            icon: Icons.hourglass_empty,
            title: 'Esperando permiso',
            detail: waiting,
          ),
        ],
      );
    }
    return ListView(
      children: [
        EmptyState(
          icon: Icons.error_outline,
          title: title,
          detail: restoreFailureText(failure),
          actionLabel: 'Reintentar',
          onAction: retry,
        ),
      ],
    );
  }

  Widget _listBody() {
    final failure = _listFailure;
    if (failure != null) {
      return _readFailure(
        failure: failure,
        title: 'No se pudieron cargar las copias',
        retry: _loadList,
      );
    }
    final points = _points;
    if (points == null) return _loadingBody();
    if (points.isEmpty) {
      return ListView(
        children: [
          EmptyState(
            icon: Icons.history,
            title: 'Sin copias todavía',
            detail: 'Aún no hay copias de este agente. Se guarda una al empezar cada turno.',
            actionLabel: 'Actualizar',
            onAction: _loadList,
          ),
        ],
      );
    }
    final now = DateTime.now().millisecondsSinceEpoch;
    return ListView(
      padding: const EdgeInsets.symmetric(vertical: Spacing.sm),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            Spacing.md,
            Spacing.lg,
            Spacing.sm,
          ),
          child: Text(widget.title, style: context.text.title),
        ),
        for (final point in points)
          RestorePointTile(
            point: point,
            now: now,
            onTap: () => _openPoint(point),
          ),
      ],
    );
  }

  Widget _detailBody() {
    final point = _point;
    final failure = _detailFailure;
    if (point == null) return const SizedBox.shrink();
    if (failure != null) {
      return _readFailure(
        failure: failure,
        title: 'No se pudieron cargar los cambios',
        retry: () => _openPoint(point),
      );
    }
    final changes = _changes;
    if (changes == null) return _loadingBody();
    final excluded = excludedSentence(changes.excluded);
    final now = DateTime.now().millisecondsSinceEpoch;
    return ListView(
      padding: _pagePadding,
      children: [
        Text('Cambios de esta copia', style: context.text.title),
        const SizedBox(height: Spacing.xs),
        Text(
          '${pointLabel(point)} · ${pointTime(point.createdMs)} · '
          '${pointAge(point.createdMs, now)}',
          style: context.text.bodySecondary,
        ),
        const SizedBox(height: Spacing.md),
        ..._changeRows(changes),
        if (excluded != null) ...[
          const SizedBox(height: Spacing.md),
          Text(excluded, style: context.text.small),
        ],
        RestoreActions(
          children: [
            OutlinedButton(
              style: restoreTouch,
              onPressed: _loadList,
              child: const Text('Volver a la lista'),
            ),
            if (changes.changes.isNotEmpty)
              FilledButton(
                style: restoreTouch,
                onPressed: _confirm,
                child: const Text('Volver a este punto'),
              ),
          ],
        ),
      ],
    );
  }

  List<Widget> _changeRows(RestoreChanges changes) {
    if (changes.changes.isEmpty) {
      return [
        Text(
          'Esta copia coincide con la carpeta de ahora: no hay nada que volver a poner.',
          style: context.text.bodySecondary,
        ),
      ];
    }
    return [
      for (final change in changes.changes) RestoreChangeTile(change: change),
    ];
  }

  Widget _confirmBody() {
    final changes = _changes;
    if (changes == null) return _loadingBody();
    return ListView(
      padding: _pagePadding,
      children: [
        Text('Volver a este punto', style: context.text.title),
        const SizedBox(height: Spacing.sm),
        Text(confirmText(changes.changes.length), style: context.text.body),
        const SizedBox(height: Spacing.sm),
        Text(restoreWarning, style: context.text.small),
        _slot(),
        RestoreActions(
          children: [
            OutlinedButton(
              style: restoreTouch,
              onPressed: _busy ? null : _showDetail,
              child: const Text('Volver'),
            ),
            FilledButton(
              style: restoreTouch,
              onPressed: _busy ? null : _restore,
              child: Text(_busy ? 'Volviendo…' : 'Volver a este punto'),
            ),
          ],
        ),
      ],
    );
  }

  Widget _doneBody() {
    final done = _done;
    if (done == null) return const SizedBox.shrink();
    return ListView(
      padding: _pagePadding,
      children: [
        Text('Hecho.', style: context.text.title),
        const SizedBox(height: Spacing.sm),
        Text(doneText(done), style: context.text.body),
        const SizedBox(height: Spacing.md),
        Text('Si no era lo que querías:', style: context.text.bodySecondary),
        _slot(),
        RestoreActions(
          children: [
            FilledButton(
              style: restoreTouch,
              onPressed: _busy ? null : _undo,
              child: Text(_busy ? 'Deshaciendo…' : 'Deshacer'),
            ),
            OutlinedButton(
              style: restoreTouch,
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Cerrar'),
            ),
          ],
        ),
      ],
    );
  }

  Widget _undoneBody() => ListView(
    padding: _pagePadding,
    children: [
      Text('Deshecho.', style: context.text.title),
      const SizedBox(height: Spacing.sm),
      Text(
        'La carpeta vuelve a estar como estaba antes de volver a esa copia.',
        style: context.text.body,
      ),
      RestoreActions(
        children: [
          FilledButton(
            style: restoreTouch,
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cerrar'),
          ),
        ],
      ),
    ],
  );

  /// Where a refused restore or undo says why; empty until one does.
  Widget _slot() {
    final failure = _slotFailure;
    if (failure == null) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: Spacing.md),
      child: RestoreNotice(
        text: isReadForbidden(failure)
            ? restoreGrantLede
            : restoreFailureText(failure),
        alert: true,
      ),
    );
  }
}
