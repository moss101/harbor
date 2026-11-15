import 'dart:convert';
import 'dart:io';

import '../build_info.dart';
import 'auth_service.dart';

/// "Report this answer" (Google Play AI-generated content policy; App
/// Review 1.2). The ONE flow that sends content off the device, and only
/// what the person confirms in the Report sheet: the answer, the reason,
/// an optional comment and — if they leave it on — their question.
class ReportService {
  ReportService(this.auth, {Uri? endpoint, HttpClient Function()? client})
      : endpoint = endpoint ?? defaultEndpoint,
        _client = client ?? HttpClient.new;

  static final defaultEndpoint = Uri.parse(
      'https://harbor-lm-reports.mohsinmustafa.workers.dev/v1/reports');

  static const reasons = [
    'offensive',
    'harmful',
    'sexual',
    'hateful',
    'illegal',
    'misinformation',
    'other',
  ];

  final HarborAuth auth;
  final Uri endpoint;
  final HttpClient Function() _client;

  /// Sends the report; returns its id. Throws on any failure.
  Future<String> send({
    required String reason,
    required String answer,
    String? question,
    String? comment,
    String? modelId,
  }) async {
    final token = await auth.idToken();
    if (token == null) throw StateError('not signed in');
    final client = _client()..connectionTimeout = const Duration(seconds: 15);
    try {
      final req = await client.postUrl(endpoint);
      req.headers
        ..set(HttpHeaders.authorizationHeader, 'Bearer $token')
        ..contentType = ContentType.json;
      req.write(jsonEncode({
        'reason': reason,
        'answer': answer,
        if (question != null && question.isNotEmpty) 'question': question,
        if (comment != null && comment.isNotEmpty) 'comment': comment,
        if (modelId != null) 'model_id': modelId,
        'app_version': harborAppVersion,
        'platform': Platform.operatingSystem,
      }));
      final res = await req.close().timeout(const Duration(seconds: 30));
      final body = await res.transform(utf8.decoder).join();
      if (res.statusCode != 201) {
        throw HttpException('report rejected (${res.statusCode})',
            uri: endpoint);
      }
      return (jsonDecode(body) as Map)['id'] as String;
    } finally {
      client.close();
    }
  }
}
