// Response cache keys include tenant identity to isolate organizations.
export async function responseCacheKey(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return `${tenant.id}:${request.path}`;
}
