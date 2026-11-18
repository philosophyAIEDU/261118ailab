// 다운로드 버튼: GitHub Releases에서 최신 설치 파일을 찾아 연결한다.
// 찾지 못하면(공개 전·비공개 저장소·네트워크 오류) 정직하게 "준비 중"으로 표시한다.
(function () {
  var cfg = window.SITE_CONFIG || {};
  var repo = cfg.RELEASES_REPO;
  var releasesPage = 'https://github.com/' + repo + '/releases';
  var ua = navigator.userAgent || '';
  var os = /Mac|iPhone|iPad/.test(ua) ? 'mac' : /Windows/.test(ua) ? 'windows' : 'other';
  function $(s) { return document.querySelector(s); }
  function $$(s) { return Array.prototype.slice.call(document.querySelectorAll(s)); }
  function fmt(n) { return n > 1048576 ? (n / 1048576).toFixed(1) + 'MB' : Math.round(n / 1024) + 'KB'; }

  // 방문자 운영체제에 맞는 버튼을 먼저 강조
  $$('[data-os]').forEach(function (el) { if (el.dataset.os === os) el.classList.add('is-mine'); });

  function setButtons(info) {
    $$('[data-dl]').forEach(function (a) {
      var asset = info && info[a.dataset.dl];
      var sub = a.querySelector('.dl-sub');
      if (asset) {
        a.href = asset.url;
        a.classList.remove('is-pending');
        if (sub) sub.textContent = 'v' + info.version + (asset.size ? ' · ' + fmt(asset.size) : '');
      } else {
        a.href = releasesPage;
        a.classList.add('is-pending');
        if (sub) sub.textContent = info ? '이 운영체제용 파일 준비 중' : '첫 버전 준비 중';
      }
    });
    $$('[data-version]').forEach(function (el) {
      el.textContent = info
        ? '최신 버전 v' + info.version + (info.date ? ' · ' + info.date : '') + (info.prerelease ? ' (시험판)' : '')
        : '설치 파일 준비 중입니다. 공개되면 이 버튼으로 바로 내려받을 수 있습니다.';
    });
    var list = $('#asset-list');
    if (list) {
      list.innerHTML = '';
      (info ? info.all : []).forEach(function (f) {
        var li = document.createElement('li');
        var a = document.createElement('a');
        a.href = f.url;
        a.textContent = f.name;
        li.appendChild(a);
        if (f.size) li.appendChild(document.createTextNode(' · ' + fmt(f.size)));
        list.appendChild(li);
      });
    }
    var notes = $('#release-link');
    if (notes) notes.href = info && info.page ? info.page : releasesPage;
  }

  function pick(rel) {
    var assets = (rel.assets || []).filter(function (a) { return !/\.sig$|latest\.json$|\.tar\.gz$/.test(a.name); });
    function find(re) { return assets.filter(function (a) { return re.test(a.name); })[0]; }
    function toA(a) { return a && { url: a.browser_download_url, size: a.size, name: a.name }; }
    return {
      version: (rel.tag_name || '').replace(/^v/, ''),
      date: (rel.published_at || '').slice(0, 10),
      prerelease: rel.prerelease,
      page: rel.html_url,
      windows: toA(find(/setup\.exe$|\.msi$/i)),
      mac: toA(find(/\.dmg$/i)),
      all: assets.filter(function (a) { return /\.(exe|msi|dmg)$/i.test(a.name); }).map(toA),
    };
  }

  // 화면 둘러보기 탭
  $$('.shot-tab').forEach(function (b) {
    b.addEventListener('click', function () {
      $$('.shot-tab').forEach(function (x) { x.setAttribute('aria-selected', String(x === b)); });
      var img = $('#shot-img');
      img.src = b.dataset.src;
      img.alt = b.dataset.alt;
      $('#shot-caption').textContent = b.dataset.caption;
    });
  });

  var m = cfg.MANUAL || {};
  if (m.version && (m.windows || m.mac)) {
    setButtons({ version: m.version, windows: m.windows && { url: m.windows }, mac: m.mac && { url: m.mac }, all: [], page: m.notes });
    return;
  }
  setButtons(null);
  fetch('https://api.github.com/repos/' + repo + '/releases?per_page=15', { headers: { Accept: 'application/vnd.github+json' } })
    .then(function (r) { return r.ok ? r.json() : []; })
    .then(function (rels) {
      var usable = (rels || [])
        .filter(function (r) { return !r.draft && r.tag_name !== 'updater-beta'; })
        .map(pick)
        .filter(function (r) { return r.windows || r.mac; });
      var stable = usable.filter(function (r) { return !r.prerelease; })[0];
      setButtons(stable || usable[0] || null);
    })
    .catch(function () { setButtons(null); });
})();
