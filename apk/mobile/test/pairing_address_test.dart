import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pairing_address.dart';

void main() {
  // input, expected. A bare host follows the scheme rule; a typed scheme wins.
  const table = <(String, String)>[
    // Bare loopback and tailnet hosts are plain http.
    ('127.0.0.1:18878', 'http://127.0.0.1:18878'),
    ('localhost:8787', 'http://localhost:8787'),
    ('100.64.0.1', 'http://100.64.0.1'),
    ('100.64.0.9:8787', 'http://100.64.0.9:8787'),
    ('100.127.255.255:8787', 'http://100.127.255.255:8787'),
    ('[::1]:8787', 'http://[::1]:8787'),
    // Private addresses have no certificate to check, so they are https too.
    ('10.0.2.2:8788', 'https://10.0.2.2:8788'),
    ('10.1.2.3', 'https://10.1.2.3'),
    ('192.168.1.20:8788', 'https://192.168.1.20:8788'),
    ('172.16.0.5', 'https://172.16.0.5'),
    ('172.31.255.1:8788', 'https://172.31.255.1:8788'),
    // Everything else is https, including the edges just outside the ranges.
    ('172.15.0.1:8788', 'https://172.15.0.1:8788'),
    ('172.32.0.1:8788', 'https://172.32.0.1:8788'),
    ('100.63.0.1:8788', 'https://100.63.0.1:8788'),
    ('100.128.0.1:8788', 'https://100.128.0.1:8788'),
    ('203.0.113.5:8788', 'https://203.0.113.5:8788'),
    ('pc.tailnet.ts.net:8788', 'https://pc.tailnet.ts.net:8788'),
    ('pc.example.com', 'https://pc.example.com'),
    // Surrounding spaces and trailing slashes are dropped first.
    ('  100.64.0.9:8787/  ', 'http://100.64.0.9:8787'),
    ('pc.tailnet.ts.net:8788//', 'https://pc.tailnet.ts.net:8788'),
    // A scheme the user typed is kept as written.
    ('http://pc.example.com', 'http://pc.example.com'),
    ('http://192.168.1.20:8788', 'http://192.168.1.20:8788'),
    ('https://127.0.0.1:8788', 'https://127.0.0.1:8788'),
    ('http://203.0.113.5:8788', 'http://203.0.113.5:8788'),
    ('HTTP://100.64.0.1:8788', 'HTTP://100.64.0.1:8788'),
    // Blank stays blank.
    ('', ''),
    ('   ', ''),
  ];

  // url, debug build, refused. Only an explicit http:// to a private IP in a
  // release build is refused; the tailnet, loopback and https stay as they are.
  const cleartext = <(String, bool, bool)>[
    ('http://192.168.1.20:8788', false, true),
    ('http://10.0.2.2:8788', false, true),
    ('http://172.16.0.1', false, true),
    ('http://172.31.255.1:8788', false, true),
    ('http://172.32.0.1', false, false),
    ('https://192.168.1.20:8788', false, false),
    ('http://192.168.1.20:8788', true, false),
    ('http://100.64.0.9:8787', false, false),
    ('http://127.0.0.1:18878', false, false),
    ('http://pc.example.com', false, false),
  ];

  group('normaliseUrl', () {
    for (final (input, expected) in table) {
      test('"$input" becomes "$expected"', () {
        expect(PairingAddress.normaliseUrl(input), expected);
      });
    }
  });

  group('privateCleartextProblem', () {
    for (final (url, debug, refused) in cleartext) {
      test('"$url" in ${debug ? 'debug' : 'release'} is '
          '${refused ? 'refused' : 'accepted'}', () {
        final problem = PairingAddress.privateCleartextProblem(
          url,
          allowLoopback: debug,
        );
        if (refused) {
          expect(problem, 'Usa la dirección de la tailnet (100.x) o https');
        } else {
          expect(problem, isNull);
        }
      });
    }
  });
}
