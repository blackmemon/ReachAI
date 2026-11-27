# 0007 No Redis, no DuckDB

Status: accepted. The standard adds a part only when a requirement needs it.

Sessions, rate limits and the job queue are in PostgreSQL. This is enough for the first release. Redis is added when session reads or rate limit counters show up as a cost in the load test. DuckDB is for analytics and the reports here are simple counts, so it is not used.
