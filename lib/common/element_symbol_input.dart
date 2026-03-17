import 'dart:async';

/// Maps chemical element symbols to their atomic numbers (H=1 through Cm=96).
const Map<String, int> elementSymbolToNumber = {
  'H': 1,
  'He': 2,
  'Li': 3,
  'Be': 4,
  'B': 5,
  'C': 6,
  'N': 7,
  'O': 8,
  'F': 9,
  'Ne': 10,
  'Na': 11,
  'Mg': 12,
  'Al': 13,
  'Si': 14,
  'P': 15,
  'S': 16,
  'Cl': 17,
  'Ar': 18,
  'K': 19,
  'Ca': 20,
  'Sc': 21,
  'Ti': 22,
  'V': 23,
  'Cr': 24,
  'Mn': 25,
  'Fe': 26,
  'Co': 27,
  'Ni': 28,
  'Cu': 29,
  'Zn': 30,
  'Ga': 31,
  'Ge': 32,
  'As': 33,
  'Se': 34,
  'Br': 35,
  'Kr': 36,
  'Rb': 37,
  'Sr': 38,
  'Y': 39,
  'Zr': 40,
  'Nb': 41,
  'Mo': 42,
  'Tc': 43,
  'Ru': 44,
  'Rh': 45,
  'Pd': 46,
  'Ag': 47,
  'Cd': 48,
  'In': 49,
  'Sn': 50,
  'Sb': 51,
  'Te': 52,
  'I': 53,
  'Xe': 54,
  'Cs': 55,
  'Ba': 56,
  'La': 57,
  'Ce': 58,
  'Pr': 59,
  'Nd': 60,
  'Pm': 61,
  'Sm': 62,
  'Eu': 63,
  'Gd': 64,
  'Tb': 65,
  'Dy': 66,
  'Ho': 67,
  'Er': 68,
  'Tm': 69,
  'Yb': 70,
  'Lu': 71,
  'Hf': 72,
  'Ta': 73,
  'W': 74,
  'Re': 75,
  'Os': 76,
  'Ir': 77,
  'Pt': 78,
  'Au': 79,
  'Hg': 80,
  'Tl': 81,
  'Pb': 82,
  'Bi': 83,
  'Po': 84,
  'At': 85,
  'Rn': 86,
  'Fr': 87,
  'Ra': 88,
  'Ac': 89,
  'Th': 90,
  'Pa': 91,
  'U': 92,
  'Np': 93,
  'Pu': 94,
  'Am': 95,
  'Cm': 96,
};

/// Reverse map: atomic number -> element symbol (e.g., 6 -> "C", 14 -> "Si").
final Map<int, String> elementNumberToSymbol = {
  for (final entry in elementSymbolToNumber.entries) entry.value: entry.key,
};

/// Uppercase letters that begin a two-character element symbol.
final Set<String> _twoCharPrefixes = elementSymbolToNumber.keys
    .where((s) => s.length == 2)
    .map((s) => s[0])
    .toSet();

/// Accumulates keyboard letter input and matches against chemical element
/// symbols. Handles disambiguation between one-char and two-char symbols via
/// a timeout (e.g. "S" alone -> Sulfur after 400 ms, "S"+"i" fast -> Silicon).
class ElementSymbolAccumulator {
  final void Function(int atomicNumber, String symbol) onMatch;
  final Duration timeout;

  String _buffer = '';
  Timer? _timer;

  ElementSymbolAccumulator({
    required this.onMatch,
    this.timeout = const Duration(milliseconds: 400),
  });

  /// Feed a single letter (a-z / A-Z) into the accumulator.
  /// Returns true if the letter was consumed as part of element input.
  bool handleLetter(String letter) {
    assert(letter.length == 1);
    _cancelTimer();

    if (_buffer.isEmpty) {
      _buffer = letter.toUpperCase();
    } else {
      _buffer += letter.toLowerCase();
    }

    if (_buffer.length == 1) {
      final match = elementSymbolToNumber[_buffer];
      final hasExtensions = _twoCharPrefixes.contains(_buffer);

      if (match != null && !hasExtensions) {
        // Unambiguous single-char element (V, U, W)
        _commit(_buffer, match);
      } else if (hasExtensions) {
        // Could be start of a two-char symbol — wait for more input or timeout
        _startTimer();
      } else {
        // Not a valid element start (e.g. J, Q)
        _clear();
        return false;
      }
    } else if (_buffer.length >= 2) {
      final twoChar = _buffer.substring(0, 2);
      final twoCharMatch = elementSymbolToNumber[twoChar];
      if (twoCharMatch != null) {
        _commit(twoChar, twoCharMatch);
      } else {
        // Two chars don't match; try first char alone as fallback
        final oneChar = _buffer[0];
        final oneCharMatch = elementSymbolToNumber[oneChar];
        if (oneCharMatch != null) {
          _commit(oneChar, oneCharMatch);
        } else {
          _clear();
          return false;
        }
      }
    }

    return true;
  }

  /// Clear any pending input. Call this when a non-letter key is pressed
  /// or when the context changes (e.g. tool switch).
  void reset() {
    _clear();
  }

  void dispose() {
    _cancelTimer();
  }

  void _startTimer() {
    _timer = Timer(timeout, _onTimeout);
  }

  void _onTimeout() {
    if (_buffer.length == 1) {
      final match = elementSymbolToNumber[_buffer];
      if (match != null) {
        _commit(_buffer, match);
        return;
      }
    }
    _clear();
  }

  void _commit(String symbol, int atomicNumber) {
    _clear();
    onMatch(atomicNumber, symbol);
  }

  void _clear() {
    _buffer = '';
    _cancelTimer();
  }

  void _cancelTimer() {
    _timer?.cancel();
    _timer = null;
  }
}
