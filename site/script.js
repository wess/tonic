const examples = {
  script: 'tonic run hello.exs\ntonic build hello.exs -o hello\n./hello',
  mix: 'mix deps.get\nmix tonic.check\nmix tonic.build\nmix tonic.run'
};
let selected = 'script';
const tabs = [...document.querySelectorAll('[data-example]')];
function selectExample(tab) {
  selected = tab.dataset.example;
  tabs.forEach(item => {
    const active = item === tab;
    item.setAttribute('aria-selected', String(active));
    item.tabIndex = active ? 0 : -1;
    document.getElementById(item.getAttribute('aria-controls')).hidden = !active;
  });
}
tabs.forEach((tab, index) => {
  tab.addEventListener('click', () => selectExample(tab));
  tab.addEventListener('keydown', event => {
    let next;
    if (event.key === 'ArrowRight') next = tabs[(index + 1) % tabs.length];
    if (event.key === 'ArrowLeft') next = tabs[(index + tabs.length - 1) % tabs.length];
    if (event.key === 'Home') next = tabs[0];
    if (event.key === 'End') next = tabs.at(-1);
    if (!next) return;
    event.preventDefault();
    selectExample(next);
    next.focus();
  });
});
document.querySelectorAll('[data-platform]').forEach(button => {
  button.addEventListener('click', () => {
    document.querySelectorAll('[data-platform]').forEach(item => {
      item.setAttribute('aria-pressed', String(item === button));
    });
    const platform = button.dataset.platform === 'linux' ? 'linuxx64' : 'macosarm64';
    document.getElementById('install-code').textContent = `tar -xzf tonic0.0.1.${platform}.tar.gz\ncd tonic0.0.1\nbash install.sh "$HOME/.local"\nexport PATH="$HOME/.local/bin:$PATH"\ntonic run hello.exs`;
  });
});
document.querySelectorAll('[data-copy]').forEach(button => {
  button.addEventListener('click', async () => {
    const value = button.dataset.copy === 'example' ? examples[selected] : document.getElementById('install-code').textContent;
    const status = document.getElementById('copy-status');
    try {
      await navigator.clipboard.writeText(value);
      status.textContent = 'Commands copied to clipboard.';
      const label = button.innerHTML;
      button.textContent = 'Copied!';
      button.disabled = true;
      setTimeout(() => { button.innerHTML = label; button.disabled = false; }, 1600);
    } catch {
      status.textContent = 'Copy unavailable. Select and copy the commands above.';
      button.textContent = 'Select commands above';
    }
  });
});
