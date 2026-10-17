// Automatic language redirect for documentation site (GitHub Pages)
// Inspects browser preferred languages and redirects if a translation is available.
(function () {
  var path = window.location.pathname;
  // Only auto-redirect on first entry to the root or home page
  var isHome = path.endsWith('/') || path.endsWith('/index.html') || path.endsWith('/mist/') || path === '/' || path === '/mist';
  if (isHome && !sessionStorage.getItem('mist_lang_selected')) {
    var navLang = (navigator.languages && navigator.languages[0]) || navigator.language || '';
    var code = navLang.toLowerCase().split('-')[0];
    var target = null;
    if (code === 'pl') target = 'i18n/README.pl/';
    else if (code === 'de') target = 'i18n/README.de/';
    else if (code === 'es') target = 'i18n/README.es/';
    else if (code === 'zh') target = 'i18n/README.zh-CN/';
    else if (code === 'ja') target = 'i18n/README.ja/';

    if (target) {
      sessionStorage.setItem('mist_lang_selected', '1');
      var base = path.replace(/\/index\.html$/, '').replace(/\/?$/, '/');
      window.location.replace(base + target);
    }
  }
})();
