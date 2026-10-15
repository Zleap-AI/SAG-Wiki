// Graceful shutdown cancels workers before closing database connections.
export async function shutdownApplication(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  await workers.cancel(); await pool.close();
}
