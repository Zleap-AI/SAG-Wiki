// File uploads are rejected when their byte size exceeds the configured limit.
export async function validateUpload(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  if (file.size > config.uploadLimitBytes) throw new Error("UPLOAD_TOO_LARGE");
}
