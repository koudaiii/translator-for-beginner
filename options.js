const keyInput = document.getElementById('key');
const regionInput = document.getElementById('region');
const status = document.getElementById('status');

chrome.storage.local.get(['key', 'region']).then(({ key, region }) => {
  keyInput.value = key || '';
  regionInput.value = region || '';
});

document.getElementById('save').addEventListener('click', async () => {
  await chrome.storage.local.set({
    key: keyInput.value.trim(),
    region: regionInput.value.trim(),
  });
  status.textContent = '保存しました';
  setTimeout(() => { status.textContent = ''; }, 2000);
});
