// Incoming webhook signatures are validated with a timing-safe comparison.
export async function verifyWebhook(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return timingSafeEqual(signature, hmac(body, config.webhookSecret));
}
