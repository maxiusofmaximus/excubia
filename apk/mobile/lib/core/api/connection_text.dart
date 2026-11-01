import 'connection_failure.dart';

/// What the user reads for a connection failure.
String connectionText(ConnectionFailure failure) => switch (failure) {
  ConnectionFailure.noRoute => 'No llego al PC. ¿Está encendido y en la misma red? Si usas una VPN, compruébala.',
  ConnectionFailure.hostDown => 'El PC responde, pero el servicio está parado. Arráncalo en el PC e inténtalo de nuevo.',
  ConnectionFailure.timeout =>
    'El PC tardó demasiado en responder. Vuelve a intentarlo.',
  ConnectionFailure.identityMismatch =>
    'El PC ya no es el que emparejaste. Vuelve a emparejarlo desde el PC.',
};

/// The short label for the status bar; without a failure, the generic one.
String connectionLabel(ConnectionFailure? failure) => switch (failure) {
  null => 'Sin conexión',
  ConnectionFailure.noRoute => 'Sin ruta al PC',
  ConnectionFailure.hostDown => 'PC sin servicio',
  ConnectionFailure.timeout => 'Lento',
  ConnectionFailure.identityMismatch => 'PC distinto',
};
