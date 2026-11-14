// Explicit post-merge administrator migration. Never invoked by a workflow.
// Dry-run by default; no secrets are read, printed or rotated.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {validateEnvironment} from './release.mjs';

const repository = 'Zleap-AI/SAG-Wiki';
export const releaseApiPath = path => `repos/${repository}${path ? `/${path}` : ''}`;

export function assertNoPendingReleaseDeployments(deployments) {
  assert.ok(!deployments.some(deployment => deployment.environment?.name === 'release'),
    'Resolve or cancel existing pending release deployments before removing reviewer requirements');
}

function main() {
  const args = process.argv.slice(2);
  assert.ok(args.length === 0 || (args.length === 1 && args[0] === '--apply'), 'Use migrate-release-environment.mjs [--apply]');
  const request = (path, method, body, paginate = false) => {
    const result = spawnSync('gh', ['api', ...(paginate ? ['--paginate', '--slurp'] : []), ...(method ? ['--method', method] : []), releaseApiPath(path),
      ...(body ? ['--input', '-'] : [])], {encoding: 'utf8', input: body && JSON.stringify(body), maxBuffer: 64 * 1024 * 1024, timeout: 120000});
    assert.equal(result.status, 0, `GitHub request failed: ${result.stderr?.trim() || result.error?.message}`);
    return JSON.parse(result.stdout);
  };
  const project = request('');
  assert.equal(project.permissions?.admin, true, 'Repository Admin is required for this explicit migration');
  const environment = request('environments/release');
  const policies = request('environments/release/deployment-branch-policies').branch_policies;
  assert.ok(policies.length > 0 && policies.every(policy => policy.type === 'tag' && policy.name === 'v*'), 'Only existing v* tag restrictions may be preserved');
  assert.equal(environment.can_admins_bypass, false, 'Disable environment administrator bypass in repository Settings before migration');
  const target = {wait_timer: 0, reviewers: [], prevent_self_review: false,
    deployment_branch_policy: {protected_branches: false, custom_branch_policies: true}};
  console.log(JSON.stringify({mode: args.includes('--apply') ? 'apply' : 'dry-run', repository, environment: 'release',
    change: 'Remove reviewer and timer requirements; preserve v* tag restriction, disabled admin bypass and all existing secrets.',
    current_reviewers: environment.protection_rules?.find(rule => rule.type === 'required_reviewers')?.reviewers?.length ?? 0,
    current_wait_timer: environment.protection_rules?.find(rule => rule.type === 'wait_timer')?.wait_timer ?? 0}, null, 2));
  if (args.includes('--apply')) {
    const desktopRuns = request('actions/workflows/desktop-release.yml/runs?per_page=100', undefined, undefined, true).flatMap(page => page.workflow_runs);
    assert.ok(desktopRuns.every(run => run.status === 'completed'), 'Wait for or cancel all active desktop release runs before migration');
    const runs = request('actions/runs?status=waiting&per_page=100', undefined, undefined, true).flatMap(page => page.workflow_runs);
    const pending = runs.flatMap(run => request(`actions/runs/${run.id}/pending_deployments`));
    assertNoPendingReleaseDeployments(pending);
    request('environments/release', 'PUT', target);
    validateEnvironment(request('environments/release'), request('environments/release/deployment-branch-policies').branch_policies);
    console.log('Automatic release environment verified. Existing signing secrets were preserved.');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) main();
