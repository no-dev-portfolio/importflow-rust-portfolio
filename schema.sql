CREATE TABLE IF NOT EXISTS jobs (
 id uuid PRIMARY KEY, tenant text NOT NULL, label text NOT NULL,
 request_key text NOT NULL, content_hash text NOT NULL, csv text NOT NULL,
 simulate_failures integer NOT NULL CHECK(simulate_failures BETWEEN 0 AND 2),
 status text NOT NULL CHECK(status IN ('queued','running','completed','rejected','failed','cancelled')),
 attempts integer NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 3),
 version integer NOT NULL DEFAULT 1 CHECK(version>0),
 token uuid, lease_until timestamptz, next_attempt timestamptz NOT NULL DEFAULT now(),
 row_count integer NOT NULL DEFAULT 0, issues jsonb NOT NULL DEFAULT '[]',
 message text NOT NULL DEFAULT '', created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(tenant,request_key), UNIQUE(tenant,id)
);
CREATE INDEX IF NOT EXISTS jobs_queue ON jobs(next_attempt,created_at) WHERE status IN ('queued','running');
CREATE INDEX IF NOT EXISTS jobs_tenant ON jobs(tenant,created_at DESC,id);
CREATE TABLE IF NOT EXISTS catalog (
 tenant text NOT NULL, sku text NOT NULL, name text NOT NULL,
 quantity integer NOT NULL CHECK(quantity BETWEEN 1 AND 100000),
 unit_price bigint NOT NULL CHECK(unit_price BETWEEN 0 AND 100000000),
 job_id uuid NOT NULL, PRIMARY KEY(tenant,sku),
 FOREIGN KEY(tenant,job_id) REFERENCES jobs(tenant,id)
);
CREATE TABLE IF NOT EXISTS audit (
 id bigserial PRIMARY KEY,tenant text NOT NULL,job_id uuid NOT NULL,
 action text NOT NULL,actor text NOT NULL,created_at timestamptz NOT NULL DEFAULT now(),
 FOREIGN KEY(tenant,job_id) REFERENCES jobs(tenant,id)
);
CREATE INDEX IF NOT EXISTS audit_job ON audit(tenant,job_id,id DESC);
