import 'package:excubia/src/rust/api/state.dart';

/// One agent of the team list, with the fields the rows read.
AgentEntry agent({required AgentStatus status, String? title}) => AgentEntry(
  paneId: 'w1:p1',
  workspaceId: 'w1',
  project: 'excubia',
  agent: 'opencode',
  status: status,
  attention: 3,
  title: title,
  cwd: '/repo',
  focused: false,
  canResume: false,
  restored: false,
);
