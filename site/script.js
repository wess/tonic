const examples = {
  script: 'tonic run hello.exs\ntonic build hello.exs -o hello\n./hello',
  mix: 'mix deps.get\nmix tonic check\nmix tonic build\nmix tonic run'
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
const version = '0.1.0-beta.3';
const release = `https://github.com/wess/tonic/releases/download/v${version}`;
const methods = {
  brew: ['macOS on Apple Silicon, or Linux x86-64.', 'brew install wess/packages/tonic\ntonic --version'],
  script: ['Downloads and checks the bundle for your machine, then installs it into ~/.local.', 'curl -fsSL https://wess.io/tonic/install.sh | sh\nexport PATH="$HOME/.local/bin:$PATH"\ntonic --version'],
  deb: ['Ubuntu 24.04 or newer, x86-64.', `curl -fLO ${release}/tonic${version}.linuxx64.deb\nsudo apt install ./tonic${version}.linuxx64.deb\ntonic --version`],
  bundle: ['After downloading and verifying the bundle (use linuxx64 on Linux):', `tar -xzf tonic${version}.macosarm64.tar.gz\ncd tonic${version}\nbash install.sh "$HOME/.local"\nexport PATH="$HOME/.local/bin:$PATH"\ntonic run hello.exs`]
};
document.querySelectorAll('[data-method]').forEach(button => {
  button.addEventListener('click', () => {
    document.querySelectorAll('[data-method]').forEach(item => {
      item.setAttribute('aria-pressed', String(item === button));
    });
    const [note, code] = methods[button.dataset.method];
    document.getElementById('install-note').textContent = note;
    document.getElementById('install-code').textContent = code;
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
