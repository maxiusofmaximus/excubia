import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/onboarding/model.dart';
import 'package:herdr_notify/src/rust/api/onboarding/plan.dart';

import '../../core/state/hires_controller.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';

/// The characters a contracted person can wear: the eight with artwork. Alfred
/// is the CTO and is never hired, so he is not one of them.
const List<String> hireCharacters = [
  'bob',
  'conan',
  'dani',
  'ernesto',
  'hugo',
  'ines',
  'nicole',
  'sami',
];

/// Opens what the user can do with one contracted person. Each action asks
/// first where it must; the list is read again after the change.
Future<void> showHireActions(
  BuildContext context,
  HiresController hires,
  Hire hire,
) {
  return showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (_) => _HireActions(hires: hires, hire: hire),
  );
}

class _HireActions extends StatelessWidget {
  const _HireActions({required this.hires, required this.hire});

  final HiresController hires;
  final Hire hire;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            minVerticalPadding: Spacing.md,
            leading: Icon(Icons.edit_outlined, color: c.accentText),
            title: Text('Renombrar', style: context.text.body),
            onTap: () => _rename(context),
          ),
          ListTile(
            minVerticalPadding: Spacing.md,
            leading: Icon(Icons.face_outlined, color: c.accentText),
            title: Text('Cambiar avatar', style: context.text.body),
            onTap: () => _changeAvatar(context),
          ),
          ListTile(
            minVerticalPadding: Spacing.md,
            leading: Icon(Icons.person_remove_outlined, color: c.accentText),
            title: Text('Despedir', style: context.text.body),
            onTap: () => _dismiss(context),
          ),
        ],
      ),
    );
  }

  Future<void> _rename(BuildContext context) async {
    final navigator = Navigator.of(context);
    final name = await showDialog<String>(
      context: context,
      builder: (_) => _RenameDialog(hireId: hire.id, initial: hire.name),
    );
    if (name == null) return;
    if (name != hire.name) await hires.rename(hire, name);
    navigator.pop();
  }

  Future<void> _changeAvatar(BuildContext context) async {
    final navigator = Navigator.of(context);
    final choice = await showDialog<({String? characterId})>(
      context: context,
      builder: (_) => _AvatarDialog(name: hire.name, current: hire.avatar),
    );
    if (choice == null) return;
    if (choice.characterId != hire.avatar) {
      await hires.changeAvatar(hire, choice.characterId);
    }
    navigator.pop();
  }

  Future<void> _dismiss(BuildContext context) async {
    final navigator = Navigator.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) {
        final c = dialogContext.colors;
        return AlertDialog(
          title: Text(
            '¿Despedir a ${hire.name}? Dejará de formar parte de tu equipo',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, false),
              child: const Text('Cancelar'),
            ),
            FilledButton(
              style: FilledButton.styleFrom(
                backgroundColor: c.statusBlocked,
                foregroundColor: c.onAttention,
              ),
              onPressed: () => Navigator.pop(dialogContext, true),
              child: const Text('Despedir'),
            ),
          ],
        );
      },
    );
    if (confirmed != true) return;
    await hires.dismiss(hire);
    navigator.pop();
  }
}

/// The new name. The rules are the Rust plan's: the same check answers here, so
/// a bad name stays in the dialog with its reason.
class _RenameDialog extends StatefulWidget {
  const _RenameDialog({required this.hireId, required this.initial});

  final String hireId;
  final String initial;

  @override
  State<_RenameDialog> createState() => _RenameDialogState();
}

class _RenameDialogState extends State<_RenameDialog> {
  late final TextEditingController _field = TextEditingController(
    text: widget.initial,
  );
  String? _problem;

  @override
  void dispose() {
    _field.dispose();
    super.dispose();
  }

  void _save() {
    final name = _field.text.trim();
    final problem = planHireRename(hireId: widget.hireId, name: name).problem;
    if (problem != null) {
      setState(() => _problem = problem.message);
      return;
    }
    Navigator.pop(context, name);
  }

  @override
  Widget build(BuildContext context) {
    final problem = _problem;
    return AlertDialog(
      title: const Text('Renombrar'),
      content: TextField(
        controller: _field,
        autofocus: true,
        textCapitalization: TextCapitalization.words,
        // A Text, not errorText: errorText is cut to one line with an ellipsis.
        decoration: InputDecoration(
          labelText: 'Nombre',
          error: problem == null ? null : Text(problem),
        ),
        onChanged: (_) {
          if (_problem != null) setState(() => _problem = null);
        },
        onSubmitted: (_) => _save(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancelar'),
        ),
        FilledButton(onPressed: _save, child: const Text('Guardar')),
      ],
    );
  }
}

/// The character picker: the eight characters, and «Sin personaje» for the
/// initial. The one it has now is marked.
class _AvatarDialog extends StatelessWidget {
  const _AvatarDialog({required this.name, required this.current});

  final String name;
  final String? current;

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Cambiar avatar'),
      contentPadding: const EdgeInsets.symmetric(vertical: Spacing.sm),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final id in hireCharacters)
              _AvatarOption(
                name: name,
                characterId: id,
                label: capitalized(id),
                selected: current == id,
                onTap: () => Navigator.pop(context, (characterId: id)),
              ),
            _AvatarOption(
              name: name,
              characterId: null,
              label: 'Sin personaje (inicial)',
              selected: current == null,
              onTap: () => Navigator.pop(context, (characterId: null)),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancelar'),
        ),
      ],
    );
  }
}

class _AvatarOption extends StatelessWidget {
  const _AvatarOption({
    required this.name,
    required this.characterId,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String name;
  final String? characterId;
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return ListTile(
      selected: selected,
      minVerticalPadding: Spacing.sm,
      leading: AgentAvatar(
        name: name,
        characterId: characterId,
        size: 40,
        ringed: true,
        animated: false,
        faceFromName: false,
      ),
      title: Text(label, style: context.text.body),
      trailing: selected ? Icon(Icons.check, color: c.accentText) : null,
      onTap: onTap,
    );
  }
}
