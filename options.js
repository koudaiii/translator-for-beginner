import { isValidEndpoint, normalizeEndpoint } from './ai.js';

const DEFAULT_DEPLOYMENT = 'gpt-6-luna';
const fields = ['key', 'region', 'aoaiEndpoint', 'aoaiKey', 'aoaiDeployment'];
const inputs = Object.fromEntries(fields.map((id) => [id, document.getElementById(id)]));
const status = document.getElementById('status');

chrome.storage.local.get(fields).then((saved) => {
  for (const id of fields) inputs[id].value = saved[id] || '';
  if (!inputs.aoaiDeployment.value) inputs.aoaiDeployment.value = DEFAULT_DEPLOYMENT;
});

function showStatus(message) {
  status.textContent = message;
  setTimeout(() => { status.textContent = ''; }, 4000);
}

document.getElementById('save').addEventListener('click', async () => {
  const aoaiEndpoint = normalizeEndpoint(inputs.aoaiEndpoint.value);
  if (aoaiEndpoint && !isValidEndpoint(aoaiEndpoint)) {
    showStatus('エンドポイントは https://<リソース名>.openai.azure.com の形式で入力してください');
    return;
  }
  inputs.aoaiEndpoint.value = aoaiEndpoint;
  await chrome.storage.local.set({
    key: inputs.key.value.trim(),
    region: inputs.region.value.trim(),
    aoaiEndpoint,
    aoaiKey: inputs.aoaiKey.value.trim(),
    aoaiDeployment: inputs.aoaiDeployment.value.trim() || DEFAULT_DEPLOYMENT,
  });
  showStatus('保存しました');
});
