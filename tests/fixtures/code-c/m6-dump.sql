CREATE TABLE users (id int,email text);
INSERT INTO users VALUES (1,'plain'),(2,'needle@example.com');
INSERT INTO users (email,id) VALUES ('needle',3);
COPY public.users (id,email) FROM stdin;
1	plain
2	needle@example.com
\.
INSERT INTO unknown VALUES (1,'needle');
