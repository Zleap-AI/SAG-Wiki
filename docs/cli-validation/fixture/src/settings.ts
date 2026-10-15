// Environment settings define the pool size and retry limit used by the runtime.
export async function loadSettings(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return { poolSize: Number(env.POOL_SIZE), maxDelayMs: Number(env.MAX_DELAY_MS) };
}
