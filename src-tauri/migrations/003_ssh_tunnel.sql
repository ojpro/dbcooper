-- Add SSH tunnel support fields to connections table
ALTER TABLE connections ADD COLUMN ssh_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE connections ADD COLUMN ssh_host TEXT NOT NULL DEFAULT '';
ALTER TABLE connections ADD COLUMN ssh_port INTEGER NOT NULL DEFAULT 22;
ALTER TABLE connections ADD COLUMN ssh_user TEXT NOT NULL DEFAULT '';
ALTER TABLE connections ADD COLUMN ssh_password TEXT NOT NULL DEFAULT '';
ALTER TABLE connections ADD COLUMN ssh_key_path TEXT NOT NULL DEFAULT '';
ALTER TABLE connections ADD COLUMN ssh_use_key INTEGER NOT NULL DEFAULT 0;
