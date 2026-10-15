// Authorization checks project roles against the required permission list.
export async function authorizeProject(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return project.roles.some(role => required.includes(role));
}
