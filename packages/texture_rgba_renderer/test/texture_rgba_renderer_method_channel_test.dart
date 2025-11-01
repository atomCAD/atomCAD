import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer_method_channel.dart';

void main() {
  MethodChannelTextureRgbaRenderer platform =
      MethodChannelTextureRgbaRenderer();
  const MethodChannel channel = MethodChannel('texture_rgba_renderer');

  TestWidgetsFlutterBinding.ensureInitialized();

  setUp(() {
    channel.setMockMethodCallHandler((MethodCall methodCall) async {
      return '42';
    });
  });

  tearDown(() {
    channel.setMockMethodCallHandler(null);
  });

  test('getPlatformVersion', () async {
    expect(await platform.getPlatformVersion(), '42');
  });
}
