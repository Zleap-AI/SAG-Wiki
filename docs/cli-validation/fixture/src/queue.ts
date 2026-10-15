// Workers acknowledge jobs only after successfully saving the result.
export async function completeJob(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  await results.save(job.id, output); await queue.ack(job.id);
}
