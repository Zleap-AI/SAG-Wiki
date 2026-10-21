<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from 'bits-ui';
  import { api } from '../lib/api';
  import { sdk, pickDirectory } from '../lib/platform';
  import { t, language } from '../lib/i18n';
  import { icon } from '../lib/icons';
  let folders: string[] = [], labels: Record<string, string> = {}, error = '', busy = false;
  async function refresh() {
    busy = true; error = '';
    try {
      folders = await api<string[]>('fnos_accessible_folders');
      if (folders.length) {
        const value = await api<{ status: number; result: { path: string; semanticPath: string }[] }>('fnos_convert_paths', { paths: folders, language: $language === 'en' ? 'en-US' : 'zh-CN' });
        labels = Object.fromEntries(value.result.map(item => [item.path, item.semanticPath]));
      }
    } catch (value) { error = String(value); }
    finally { busy = false; }
  }
  async function authorize() {
    try { await pickDirectory(); await refresh(); }
    catch (value) { error = String(value); }
  }
  async function openSettings() {
    error = '';
    try { await api('open_data_directory'); }
    catch (value) { error = String(value); }
  }
  onMount(() => { void refresh(); });
</script>
<section class="settings-group">
  <div class="setting-row"><div><strong>{$t('飞牛文件授权')}</strong><p>{$t('仅访问当前用户授权的目录，读取和保存时检查当前用户权限。')}</p></div><Button.Root type="button" class="outline" onclick={authorize}>{$t('授权文件夹')}</Button.Root></div>
  <div class="setting-row"><div><strong>{$t('已授权目录')}</strong><p>{$t('新增授权后，在添加文件夹中选择并建立索引。')}</p></div><Button.Root type="button" class="outline" disabled={busy} onclick={refresh}>{$t('刷新授权状态')}</Button.Root></div>
  <ul class="fnos-authorized-folders" aria-label={$t('已授权目录')}>
    {#each folders as path}
      <li class="fnos-authorized-folder">
        <span class="fnos-folder-icon" aria-hidden="true">{@html icon('folder')}</span>
        <div class="fnos-folder-details"><strong>{path.split('/').pop() || '/'}</strong><p>{labels[path] ?? $t('正在读取显示路径…')}</p></div>
      </li>
    {:else}
      <li class="fnos-folders-empty">{$t(busy ? '正在读取授权目录…' : '尚未授权文件夹。')}</li>
    {/each}
  </ul>
</section>
<section class="settings-group"><div class="setting-row"><div><strong>{$t('应用管理')}</strong><p>{$t(sdk.isStandaloneWeb ? '独立网页请先打开飞牛桌面，再进入“应用中心 → 已安装 → SAG Wiki → 设置”。' : '在飞牛应用设置中管理授权、升级或停止服务。')}</p></div><Button.Root type="button" class="outline" onclick={openSettings}>{$t(sdk.isStandaloneWeb ? '打开飞牛桌面' : '打开应用设置')}</Button.Root></div></section>
{#if error}<p class="form-error" role="alert">{error}</p>{/if}

<style>
  .fnos-authorized-folders{list-style:none;margin:0;padding:12px 16px 16px;display:grid;gap:8px}
  .fnos-authorized-folder{display:flex;align-items:flex-start;gap:12px;min-width:0;padding:12px 14px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-panel)}
  .fnos-folder-icon{display:grid;place-items:center;flex:0 0 32px;height:32px;color:var(--color-muted)}
  .fnos-folder-icon :global(.icon){width:20px;height:20px}
  .fnos-folder-details{min-width:0;overflow-wrap:anywhere}
  .fnos-folder-details strong{font-size:13px;font-weight:600;line-height:1.5}
  .fnos-folder-details p{margin:3px 0 0;color:var(--color-muted);font-size:12px;line-height:1.6}
  .fnos-folders-empty{padding:16px;border:1px dashed var(--color-rule);border-radius:8px;color:var(--color-muted);font-size:13px;text-align:center}
</style>
