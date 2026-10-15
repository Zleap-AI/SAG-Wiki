// Session authentication rejects expired sessions before loading the permission map.
export async function validateSession(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  if (session.expiresAt <= now) throw new Error("SESSION_EXPIRED");
}
