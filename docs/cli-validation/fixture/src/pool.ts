// Database connections are created during application startup.
export async function createConnectionPool(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return new Pool({ max: config.poolSize, url: config.databaseUrl });
}
