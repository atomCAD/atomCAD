import 'package:flutter_cad/structure_designer/node_network/arrangement_labels.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('arrangementRowLabels', () {
    test('only the input varies: rows name the input', () {
      final labels = arrangementRowLabels(const [
        ArrangementSides(
            inputName: 'base',
            inputType: '[Blueprint]',
            outputName: 'result',
            outputType: 'Blueprint'),
        ArrangementSides(
            inputName: 'sub',
            inputType: '[Blueprint]',
            outputName: 'result',
            outputType: 'Blueprint'),
      ]);
      expect(labels, const [
        ArrangementRowLabel('base', '[Blueprint]'),
        ArrangementRowLabel('sub', '[Blueprint]'),
      ]);
    });

    test('only the output varies: rows name the output', () {
      final labels = arrangementRowLabels(const [
        ArrangementSides(
            inputName: 'molecule',
            inputType: 'HasAtoms',
            outputName: 'result',
            outputType: 'Crystal'),
        ArrangementSides(
            inputName: 'molecule',
            inputType: 'HasAtoms',
            outputName: 'diff',
            outputType: 'Molecule'),
      ]);
      expect(labels, const [
        ArrangementRowLabel('result', 'Crystal'),
        ArrangementRowLabel('diff', 'Molecule'),
      ]);
    });

    test('both vary: rows read input → output, typed by the output', () {
      final labels = arrangementRowLabels(const [
        ArrangementSides(
            inputName: 'a',
            inputType: 'Int',
            outputName: 'result',
            outputType: 'Int'),
        ArrangementSides(
            inputName: 'b',
            inputType: 'Float',
            outputName: 'diff',
            outputType: 'Float'),
      ]);
      expect(labels, const [
        ArrangementRowLabel('a → result', 'Int'),
        ArrangementRowLabel('b → diff', 'Float'),
      ]);
    });

    test('a wire drop has one side: rows name that side', () {
      // Off an output: inputs only.
      expect(
        arrangementRowLabels(const [
          ArrangementSides(inputName: 'base', inputType: '[Blueprint]'),
          ArrangementSides(inputName: 'sub', inputType: '[Blueprint]'),
        ]),
        const [
          ArrangementRowLabel('base', '[Blueprint]'),
          ArrangementRowLabel('sub', '[Blueprint]'),
        ],
      );
      // Off an input: outputs only.
      expect(
        arrangementRowLabels(const [
          ArrangementSides(outputName: 'result', outputType: 'Molecule'),
          ArrangementSides(outputName: 'diff', outputType: 'Molecule'),
        ]),
        const [
          ArrangementRowLabel('result', 'Molecule'),
          ArrangementRowLabel('diff', 'Molecule'),
        ],
      );
    });

    test('a single row names whichever side it has', () {
      expect(
        arrangementRowLabels(const [
          ArrangementSides(outputName: 'result', outputType: 'Float'),
        ]),
        const [ArrangementRowLabel('result', 'Float')],
      );
      expect(
        arrangementRowLabels(const [
          ArrangementSides(
              inputName: 'x',
              inputType: 'Float',
              outputName: 'result',
              outputType: 'Float'),
        ]),
        const [ArrangementRowLabel('x', 'Float')],
      );
    });
  });
}
