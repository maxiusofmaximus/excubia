import 'host_client.dart';

// What a read says while the PC has not granted this device READ_STATE yet. Desktop,
// restore and «Tu equipo» all say it with these same words.

/// The wait a read reports while the PC has not granted this device READ_STATE.
const String readGrantWaitingText = 'Esperando que aprueben este dispositivo.';

/// The 403 of a read: the device still lacks the grant the route needs.
bool isReadForbidden(Object error) => error is HostError && error.status == 403;
