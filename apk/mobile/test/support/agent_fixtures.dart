import 'package:excubia/src/rust/api/state.dart';

/// One agent of the team list, with the fields the rows read.
AgentEntry agent({
  required AgentStatus status,
  String? title,
  String project = 'bench',
  String cli = 'opencode',
  bool restored = false,
  int? lastActiveMs,
}) => AgentEntry(
  paneId: 'w1:p1',
  workspaceId: 'w1',
  project: project,
  agent: cli,
  status: status,
  attention: 3,
  title: title,
  cwd: '/repo',
  focused: false,
  canResume: false,
  restored: restored,
  lastActiveMs: lastActiveMs,
);
