// Network request retries use exponential delays and a configured maximum.
export async function retryDelay(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return Math.min(config.maxDelayMs, 250 * 2 ** attempt);
}
