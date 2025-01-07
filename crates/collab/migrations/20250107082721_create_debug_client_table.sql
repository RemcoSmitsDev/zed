CREATE TABLE debug_clients (
  id BIGINT NOT NULL,
  project_id INTEGER NOT NULL,
  session_id BIGINT NOT NULL,
  capabilities INTEGER NOT NULL,
  panel_item BYTEA NOT NULL,
  PRIMARY KEY (id, project_id),
  FOREIGN KEY (project_id) REFERENCES projects (id) ON DELETE CASCADE
);

CREATE INDEX "index_debug_client_on_project_id" ON "debug_clients" ("project_id");
