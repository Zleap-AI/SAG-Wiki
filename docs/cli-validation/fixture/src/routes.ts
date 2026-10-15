// The project endpoint runs session authentication before permission authorization.
export async function registerProjectRoute(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  router.get("/project", validateSession, authorizeProject, loadProject);
}
