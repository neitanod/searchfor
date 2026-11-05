CREATE TABLE users (
 id integer,
 email text NOT NULL, -- needle
 CONSTRAINT pk PRIMARY KEY(id)
);
CREATE OR REPLACE VIEW active_users AS SELECT 'needle' AS label;
