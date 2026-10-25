import 'package:buzz/features/channels/channel.dart';
import 'package:buzz/shared/channels/channel_list_snapshot.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _pubkey = 'ABCDEF';
const _relay = 'https://relay.example';

final _channel = Channel(
  id: 'dm-1',
  name: 'DM',
  channelType: 'dm',
  labels: const ['buzz.solo'],
  visibility: 'private',
  description: '',
  topic: 'topic',
  purpose: 'purpose',
  createdBy: 'creator',
  createdAt: DateTime.utc(2026, 1, 2),
  memberCount: 2,
  lastMessageAt: DateTime.utc(2026, 1, 3),
  archivedAt: DateTime.utc(2026, 1, 4),
  participants: const ['Alice'],
  participantPubkeys: const ['alice'],
  isMember: true,
  ttlSeconds: 60,
  ttlDeadline: DateTime.utc(2026, 1, 5),
);

ChannelListSnapshot _snapshot([List<Channel>? channels]) => ChannelListSnapshot(
  channels: [
    for (final c in channels ?? [_channel]) c.toJson(),
  ],
  hiddenDmIds: const {'dm-2'},
);

void main() {
  late SharedPreferences prefs;
  late ChannelListSnapshotStorage storage;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    prefs = await SharedPreferences.getInstance();
    storage = ChannelListSnapshotStorage(prefs);
  });

  test('round-trips every channel field and hidden DMs', () async {
    final token = storage.capture(_pubkey, _relay);
    expect(await storage.write(token, _snapshot()), isTrue);

    final restored = storage.read(_pubkey, _relay)!;
    final channel = Channel.fromJson(restored.channels.single);
    expect(channel.toJson(), _channel.toJson());
    expect(channel.isArchived, isTrue);
    expect(channel.isEphemeral, isTrue);
    expect(restored.hiddenDmIds, {'dm-2'});
  });

  test('scopes by identity and relay, folding websocket origins', () async {
    await storage.write(storage.capture(_pubkey, _relay), _snapshot());

    expect(storage.read('abcdef', 'wss://RELAY.example/'), isNotNull);
    expect(storage.read('other', _relay), isNull);
    expect(storage.read(_pubkey, 'https://other.example'), isNull);
  });

  test('removal retires tokens captured before it, on any instance', () async {
    final stale = storage.capture(_pubkey, _relay);
    await storage.write(stale, _snapshot());

    await ChannelListSnapshotStorage(prefs).remove(_pubkey, _relay);

    expect(storage.read(_pubkey, _relay), isNull);
    expect(await storage.write(stale, _snapshot()), isFalse);
    expect(storage.read(_pubkey, _relay), isNull);
    final fresh = storage.capture(_pubkey, _relay);
    expect(await storage.write(fresh, _snapshot()), isTrue);
  });

  test('corrupt, unsupported and oversized data is discarded', () async {
    final key = ChannelListSnapshotStorage.key(_pubkey, _relay);
    final tooMany = List.filled(maxChannelListSnapshotChannels + 1, '{}');
    for (final raw in [
      'not json',
      '{"v":2,"channels":[],"hidden_dm_ids":[]}',
      '{"v":1,"channels":[${tooMany.join(',')}],"hidden_dm_ids":[]}',
      // Fits in UTF-16 code units, exceeds the cap in UTF-8 bytes.
      '{"v":1,"channels":[],"hidden_dm_ids":'
          '["${'é' * (maxChannelListSnapshotBytes ~/ 2 + 1)}"]}',
    ]) {
      await prefs.setString(key, raw);
      expect(storage.read(_pubkey, _relay), isNull);
      expect(prefs.containsKey(key), isFalse);
    }

    await storage.write(storage.capture(_pubkey, _relay), _snapshot());
    final oversized = [
      for (var i = 0; i <= maxChannelListSnapshotChannels; i++) _channel,
    ];
    expect(
      await storage.write(
        storage.capture(_pubkey, _relay),
        _snapshot(oversized),
      ),
      isFalse,
    );
    expect(storage.read(_pubkey, _relay), isNull);
  });
}
