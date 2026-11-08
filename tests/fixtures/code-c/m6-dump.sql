CREATE TABLE users (id int,email text);
INSERT INTO users VALUES (1,'plain'),(2,'needle@example.com');
INSERT INTO users (email,id) VALUES ('needle',3);
COPY public.users (id,email) FROM stdin;
1	plain
2	needle@example.com
\.
INSERT INTO unknown VALUES (1,'needle');
/*!50001 CREATE ALGORITHM=UNDEFINED */
/*!50013 DEFINER=`root`@`localhost` SQL SECURITY DEFINER */
/*!50001 VIEW `active_users` AS select `email` AS `email` from `users` where (`email` like '%needle%') */;
DELIMITER ;;
/*!50003 CREATE*/ /*!50020 DEFINER=`root`@`localhost`*/ /*!50003 PROCEDURE `find_users`()
BEGIN
  SELECT email FROM users WHERE email LIKE 'needle%';
END */;;
DELIMITER ;
