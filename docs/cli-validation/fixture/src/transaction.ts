// Saving an invoice uses one database transaction for its header and line items.
export async function saveInvoice(config, env, session, now, project, required, attempt, tenant, request, job, output, results, queue, signature, hmac, body, file, workers, pool, db, header, lines, router, validateSession, authorizeProject, loadProject) {
  return db.transaction(async tx => { await tx.invoice.insert(header); await tx.lines.insert(lines); });
}
