CREATE TABLE reports (
  id TEXT PRIMARY KEY,
  created_at TEXT NOT NULL,
  uid TEXT NOT NULL,
  reason TEXT NOT NULL,
  comment TEXT,
  question TEXT,
  answer TEXT NOT NULL,
  model_id TEXT,
  app_version TEXT,
  platform TEXT,
  status TEXT NOT NULL DEFAULT 'new'
);
CREATE INDEX reports_uid_time ON reports (uid, created_at);
CREATE INDEX reports_time ON reports (created_at);
