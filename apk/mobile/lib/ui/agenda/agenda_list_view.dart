import 'package:flutter/material.dart';

import '../../core/state/agenda_list_controller.dart';
import '../theme.dart';
import 'agenda_widgets.dart';

/// The body of a paged agenda list: a search field, an optional header, the
/// rows, the empty and failed states and «Cargar más». The search runs when the
/// person submits it, so typing alone sends nothing.
class AgendaListView<T> extends StatefulWidget {
  const AgendaListView({
    super.key,
    required this.list,
    required this.hint,
    required this.empty,
    required this.rowBuilder,
    this.header,
  });

  final AgendaListController<T> list;
  final String hint;
  final String empty;
  final Widget Function(BuildContext context, T item) rowBuilder;
  final Widget? header;

  @override
  State<AgendaListView<T>> createState() => _AgendaListViewState<T>();
}

class _AgendaListViewState<T> extends State<AgendaListView<T>> {
  final TextEditingController _search = TextEditingController();

  @override
  void initState() {
    super.initState();
    widget.list.search('');
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final list = widget.list;
    return ListenableBuilder(
      listenable: list,
      builder: (context, _) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(Spacing.md),
            child: TextField(
              controller: _search,
              textInputAction: TextInputAction.search,
              style: context.text.body,
              onSubmitted: list.search,
              decoration: InputDecoration(
                hintText: widget.hint,
                prefixIcon: const Icon(Icons.search),
                isDense: true,
              ),
            ),
          ),
          ?widget.header,
          if (list.failure != null)
            AgendaMessage(
              text: list.failure!,
              actionLabel: 'Reintentar',
              onAction: list.reload,
            ),
          Expanded(child: _body(context, list)),
        ],
      ),
    );
  }

  Widget _body(BuildContext context, AgendaListController<T> list) {
    final items = list.items;
    if (items.isEmpty) {
      if (list.loading) return const Center(child: CircularProgressIndicator());
      if (list.failure != null) return const SizedBox.shrink();
      return AgendaMessage(
        text: list.query.isEmpty
            ? widget.empty
            : 'Nada coincide con «${list.query}».',
      );
    }
    return ListView.builder(
      itemCount: items.length + (list.hasMore ? 1 : 0),
      itemBuilder: (context, index) {
        if (index < items.length) {
          return widget.rowBuilder(context, items[index]);
        }
        return Center(
          child: list.loading
              ? const Padding(
                  padding: EdgeInsets.all(Spacing.md),
                  child: CircularProgressIndicator(),
                )
              : TextButton(
                  onPressed: list.loadMore,
                  child: const Text('Cargar más'),
                ),
        );
      },
    );
  }
}
