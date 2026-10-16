function show(key, html) { return HIDDEN.has(key) ? '' : html; }
function sectionHeader(title, keys) {
  return keys.every(k => HIDDEN.has(k)) ? '' : `<div class="section-header"><h2>${title}</h2></div>`;
}
function section(cls, parts) {
  const html = parts.filter(Boolean).join('');
  return html ? `<div class="${cls}">${html}</div>` : '';
}
const fmt = n => n ? n.toLocaleString() : 0;
const fmtDate = ts => new Date(ts * 1000).toISOString().slice(0, 10);
function esc(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }

function mergedAuthors() {
  const byName = {};
  for (const a of Object.values(DATA.authors || {})) {
    if (!byName[a.name]) {
      byName[a.name] = { ...a, emails: [a.email], fixes: (a.fixes||[]).length, closes: (a.closes||[]).length };
    } else {
      const m = byName[a.name];
      m.emails.push(a.email);
      m.commits += a.commits; m.additions += a.additions; m.deletions += a.deletions;
      m.files_changed += a.files_changed; m.fixes += (a.fixes||[]).length; m.closes += (a.closes||[]).length;
      m.ai_assisted += a.ai_assisted;
      m.is_bot = m.is_bot || a.is_bot;
      if (a.first_commit_ts < m.first_commit_ts) m.first_commit_ts = a.first_commit_ts;
      if (a.last_commit_ts > m.last_commit_ts) m.last_commit_ts = a.last_commit_ts;
    }
  }
  return Object.values(byName).sort((a, b) => b.commits - a.commits);
}

function humanAuthors() { return mergedAuthors().filter(a => !a.is_bot); }

function buildAffiliations() {
  const KNOWN = {
    'redhat.com':'Red Hat','google.com':'Google','microsoft.com':'Microsoft',
    'ibm.com':'IBM','intel.com':'Intel','linux.intel.com':'Intel','mozilla.org':'Mozilla','mozilla.com':'Mozilla',
    'canonical.com':'Canonical','suse.com':'SUSE','suse.de':'SUSE',
    'samsung.com':'Samsung','oracle.com':'Oracle','amazon.com':'Amazon',
    'meta.com':'Meta','fb.com':'Meta','apple.com':'Apple','huawei.com':'Huawei',
    'arm.com':'Arm','nvidia.com':'NVIDIA','collabora.com':'Collabora',
    'igalia.com':'Igalia','codethink.co.uk':'Codethink',
    'endlessm.com':'Endless','endlessos.org':'Endless',
    'gnome.org':'GNOME','kde.org':'KDE','debian.org':'Debian',
    'ubuntu.com':'Ubuntu','fedoraproject.org':'Fedora','freedesktop.org':'freedesktop.org',
    'linaro.org':'Linaro','broadcom.com':'Broadcom','qualcomm.com':'Qualcomm',
    'vmware.com':'VMware','siemens.com':'Siemens','ti.com':'Texas Instruments',
    'nxp.com':'NXP','gitlab.com':'GitLab','github.com':'GitHub',
    'cloudflare.com':'Cloudflare','cisco.com':'Cisco','ericsson.com':'Ericsson',
    'nokia.com':'Nokia','sony.com':'Sony','twitter.com':'Twitter','x.com':'X',
    'shopify.com':'Shopify','netflix.com':'Netflix',
    'uber.com':'Uber','alibaba-inc.com':'Alibaba','bytedance.com':'ByteDance',
    'tencent.com':'Tencent','baidu.com':'Baidu','fujitsu.com':'Fujitsu',
    'hitachi.com':'Hitachi','renesas.com':'Renesas','sap.com':'SAP',
    'hp.com':'HP','dell.com':'Dell','lenovo.com':'Lenovo',
    'amd.com':'AMD','marvell.com':'Marvell','mediatek.com':'MediaTek',
    'microchip.com':'Microchip',
  };
  const PERSONAL = new Set([
    'gmail.com','hotmail.com','live.com','outlook.com','yahoo.com','yahoo.fr',
    'yahoo.co.uk','protonmail.com','proton.me','icloud.com','me.com','mac.com',
    'googlemail.com','aol.com','zoho.com','yandex.ru','mail.ru','gmx.com','gmx.de','pm.me',
    'fastmail.com','users.noreply.github.com','localhost','posteo.net',
    'tutanota.com','tuta.io','mailbox.org','web.de','t-online.de',
    'free.fr','laposte.net','orange.fr','wanadoo.fr','seznam.cz',
    '163.com','qq.com','126.com','sina.com','naver.com',
  ]);
  const authors = humanAuthors();
  const total = authors.reduce((s, a) => s + a.commits, 0);
  const counts = {};
  // Config overrides: an exact email wins over a domain, which wins over the built-in tables.
  // An empty company means "personal / unaffiliated".
  const lookup = key => Object.hasOwn(AFFILIATIONS, key) ? AFFILIATIONS[key] : undefined;
  const affiliationOf = emails => {
    for (const email of emails) {
      const override = lookup(email.toLowerCase());
      if (override !== undefined) return override || null;
    }
    for (const email of emails) {
      const at = email.lastIndexOf('@');
      if (at < 0) continue;
      const domain = email.slice(at + 1).toLowerCase();
      const override = lookup(domain);
      if (override !== undefined) {
        if (override) return override;
        continue;
      }
      if (PERSONAL.has(domain)) continue;
      return KNOWN[domain] || domain;
    }
    return null;
  };
  for (const a of authors) {
    const aff = affiliationOf(a.emails);
    if (aff) counts[aff] = (counts[aff] || 0) + a.commits;
  }
  const threshold = total * 0.01;
  let other = 0;
  const result = [];
  for (const [name, count] of Object.entries(counts)) {
    if (count < threshold) other += count;
    else result.push([name, count]);
  }
  result.sort((a, b) => b[1] - a[1]);
  if (other > 0) result.push(['Other', other]);
  return result;
}

const PIE_COLORS = ['#3b82f6','#10b981','#f59e0b','#8b5cf6','#ef4444','#ec4899','#14b8a6','#6366f1','#84cc16','#f43f5e'];

function donutSvg(slices, size, innerR) {
  const cx = size / 2, cy = size / 2, R = size / 2 - 4, r = innerR;
  let svg = `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">`;
  const total = slices.reduce((s, sl) => s + sl.value, 0);
  if (total === 0) return svg + '</svg>';
  let angle = -Math.PI / 2;
  slices.forEach((sl, idx) => {
    if (sl.value === 0) return;
    const frac = sl.value / total;
    const sweep = frac * Math.PI * 2;
    const x1o = cx + R * Math.cos(angle), y1o = cy + R * Math.sin(angle);
    const x1i = cx + r * Math.cos(angle), y1i = cy + r * Math.sin(angle);
    const x2o = cx + R * Math.cos(angle + sweep), y2o = cy + R * Math.sin(angle + sweep);
    const x2i = cx + r * Math.cos(angle + sweep), y2i = cy + r * Math.sin(angle + sweep);
    const large = sweep > Math.PI ? 1 : 0;
    const color = sl.color || PIE_COLORS[idx % PIE_COLORS.length];
    svg += `<path d="M${x1o},${y1o} A${R},${R} 0 ${large} 1 ${x2o},${y2o} L${x2i},${y2i} A${r},${r} 0 ${large} 0 ${x1i},${y1i} Z" fill="${color}" stroke="var(--c-surface)" stroke-width="2.5"><title>${esc(sl.label)}: ${fmt(sl.value)} (${(frac*100).toFixed(1)}%)</title></path>`;
    angle += sweep;
  });
  svg += '</svg>';
  return svg;
}

function renderVitalsAndHeatmap() {
  const authors = Object.values(DATA.authors || {});
  const totalFixes = authors.reduce((s, a) => s + (a.fixes || []).length, 0);
  const totalAI = authors.reduce((s, a) => s + (a.ai_assisted || 0), 0);
  const totalBots = authors.filter(a => a.is_bot).reduce((s, a) => s + a.commits, 0);
  const years = (DATA.yearly_commit_counts || []).map(([y]) => y);
  let lifespan = "All-time Profile";
  if (years.length) {
    const firstYear = Math.min(...years), lastYear = Math.max(...years);
    lifespan = lastYear - firstYear > 0 ? `${firstYear}–${lastYear}` : `Since ${firstYear}`;
  }
  const uniqueAuthors = new Set(authors.filter(a => !a.is_bot).map(a => a.name)).size;
  const generatedAt = new Date().toISOString().slice(0, 10);
  // Combined reports clear from_ref (combine.rs) — no single "latest commit" applies.
  const isCombined = !DATA.from_ref;
  const latestCommit = DATA.latest_commit || null;
  const latestPart = (isCombined || !latestCommit) ? '' :
    ` · Latest commit <span class="font-mono text-accent" style="font-weight:600">${latestCommit.id.slice(0, 7)}</span> at ${fmtDate(latestCommit.timestamp)}`;

  const stat = (label, value) => `<div class="stat-box">
    <div class="stat-label">${label}</div>
    <div class="font-mono stat-value">${value}</div>
  </div>`;

  return `<div class="card">
    <div class="header-row">
      <div>
        <h1 class="report-title">{{REPO_NAME}} <span class="font-mono text-accent report-range">${DATA.from_ref ? DATA.from_ref + '…' : ''}${DATA.to_ref}</span></h1>
        <div class="report-subtitle">${lifespan} · Generated ${generatedAt}${latestPart}</div>
      </div>
      <button class="theme-toggle" onclick="toggleTheme()" aria-label="Toggle theme">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/>
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
          <line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/>
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
        </svg>
        <span>Theme</span>
      </button>
    </div>
    <div class="vitals-grid">
      ${stat('Commits', `<span style="color:var(--c-accent)">${fmt(DATA.total_commits)}</span>`)}
      ${stat('Contributors', fmt(uniqueAuthors))}
      ${stat('Code Volume', `<span class="text-positive">+${fmt(DATA.total_additions)}</span> <span class="text-negative">-${fmt(DATA.total_deletions)}</span>`)}
      ${stat('Resolved Fixes', `<span style="color:#d97706">${fmt(totalFixes)}</span>`)}
      ${stat('AI Contributions', `<span style="color:#8b5cf6">${fmt(totalAI)}</span>`)}
      ${stat('Automated Bots', fmt(totalBots))}
    </div>
  </div>`;
}

function renderHeatmap() {
  const daily = DATA.daily_commit_counts || {};
  const maxCount = Math.max(...Object.values(daily), 1);
  function intensity(count) {
    if (!count) return 'var(--c-heatmap-0)';
    const t = count / maxCount;
    if (t <= 0.25) return 'var(--c-heatmap-1)';
    if (t <= 0.5) return 'var(--c-heatmap-2)';
    if (t <= 0.75) return 'var(--c-heatmap-3)';
    return 'var(--c-heatmap-4)';
  }
  const today = new Date(), weeks = 52;
  const start = new Date(today);
  start.setDate(start.getDate() - weeks * 7);
  start.setDate(start.getDate() - start.getDay());
  const monthLabels = [], cells = [];
  let lastMonth = -1;
  for (let w = 0; w <= weeks; w++) {
    const col = [];
    for (let d = 0; d < 7; d++) {
      const date = new Date(start); date.setDate(start.getDate() + w * 7 + d);
      const key = date.toISOString().slice(0, 10);
      col.push({ key, count: daily[key] || 0, future: date > today });
      if (d === 0) { const m = date.getMonth(); if (m !== lastMonth) { monthLabels.push({ week: w, label: date.toLocaleString('en', { month: 'short' }) }); lastMonth = m; } }
    }
    cells.push(col);
  }
  const sz = 12, gap = 4, step = sz + gap, lp = 36, tp = 24;
  const svgW = lp + cells.length * step, svgH = tp + 7 * step;
  const dayNames = ['', 'Mon', '', 'Wed', '', 'Fri', ''];
  let svg = `<svg viewBox="0 0 ${svgW} ${svgH}" style="width:100%;max-width:${svgW}px;display:block;">`;
  for (const { week, label } of monthLabels) svg += `<text x="${lp + week * step}" y="14" fill="var(--c-muted)" style="font-size:11px;font-weight:500;">${label}</text>`;
  for (let d = 0; d < 7; d++) if (dayNames[d]) svg += `<text x="0" y="${tp + d * step + 10}" fill="var(--c-muted)" style="font-size:11px;font-weight:500;">${dayNames[d]}</text>`;
  for (let w = 0; w < cells.length; w++)
    for (let d = 0; d < 7; d++) {
      const { key, count, future } = cells[w][d];
      if (future) continue;
      svg += `<rect x="${lp + w * step}" y="${tp + d * step}" width="${sz}" height="${sz}" rx="2" fill="${intensity(count)}"><title>${key}: ${count} commit${count !== 1 ? 's' : ''}</title></rect>`;
    }
  svg += '</svg>';
  const hc = ['var(--c-heatmap-0)','var(--c-heatmap-1)','var(--c-heatmap-2)','var(--c-heatmap-3)','var(--c-heatmap-4)'];
  return `<div class="card">
    <div class="card-title">Contribution Activity</div>
    <div class="heatmap-wrap">${svg}</div>
    <div class="heatmap-legend">
      <span class="heatmap-scale-label">Less</span>
      ${hc.map(c => `<span class="swatch" style="background:${c}"></span>`).join('')}
      <span class="heatmap-scale-label">More</span>
    </div>
  </div>`;
}

function renderAffiliationsDonut() {
  const aff = buildAffiliations();
  if (!aff.length) return '';
  const total = aff.reduce((s, a) => s + a[1], 0);
  const slices = aff.map(([label, value]) => ({ label, value }));
  const svg = donutSvg(slices, 160, 50);
  const legend = aff.map(([name, count], i) => {
    const pct = total > 0 ? (count / total * 100).toFixed(1) : '0.0';
    return `<div class="legend-row">
      <div class="legend-label">
        <span class="legend-dot" style="background:${PIE_COLORS[i % PIE_COLORS.length]}"></span>
        <span class="legend-text">${esc(name)}</span>
      </div>
      <div class="font-mono legend-value">
        <span>${fmt(count)} commits</span> <span class="legend-value-pct">${pct}%</span>
      </div>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Affiliations</div>
    <div class="donut-layout">
      <div class="donut-chart">${svg}</div>
      <div class="donut-legend donut-legend--tall">${legend}</div>
    </div>
  </div>`;
}

function renderPulse() {
  const entries = DATA.yearly_commit_counts || [];
  const years = entries.map(([y]) => String(y));
  const vals = entries.map(([, v]) => v);
  const maxV = Math.max(...vals, 1);
  if (years.length < 2) return '';
  const W = 500, H = 220;
  // Left padding scales with the tick label width so large counts don't collide with the axis title.
  const pad = { t: 16, r: 16, b: 48, l: 28 + fmt(maxV).length * 7 };
  const cw = W - pad.l - pad.r, ch = H - pad.t - pad.b;
  const barW = Math.min(Math.floor(cw / years.length) - 4, 28);
  const step = cw / years.length;
  let svg = `<svg width="100%" viewBox="0 0 ${W} ${H}" style="display:block">`;
  svg += `<text x="${12}" y="${pad.t + ch / 2}" text-anchor="middle" fill="var(--c-muted)" style="font-size:9px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em" transform="rotate(-90 12 ${pad.t + ch / 2})">Commits</text>`;
  svg += `<text x="${pad.l + cw / 2}" y="${H - 4}" text-anchor="middle" fill="var(--c-muted)" style="font-size:9px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em">Year</text>`;
  for (let i = 0; i <= 4; i++) {
    const y = pad.t + ch - (ch * i / 4);
    svg += `<line x1="${pad.l}" y1="${y}" x2="${W - pad.r}" y2="${y}" stroke="var(--c-grid)" stroke-width="1" ${i > 0 ? 'stroke-dasharray="4,4"' : ''}/>`;
    svg += `<text x="${pad.l - 8}" y="${y + 3}" text-anchor="end" fill="var(--c-muted)" style="font-size:10px;font-weight:500">${fmt(Math.round(maxV * i / 4))}</text>`;
  }
  const showEvery = years.length > 20 ? 3 : years.length > 10 ? 2 : 1;
  for (let i = 0; i < years.length; i++) {
    const x = pad.l + i * step + (step - barW) / 2;
    const h = (vals[i] / maxV) * ch;
    svg += `<rect x="${x}" y="${pad.t + ch - h}" width="${barW}" height="${h}" rx="3" fill="var(--c-accent)" opacity="0.9"><title>${years[i]}: ${fmt(vals[i])} commits</title></rect>`;
    if (i % showEvery === 0 || i === years.length - 1) svg += `<text x="${x + barW / 2}" y="${pad.t + ch + 12}" text-anchor="end" fill="var(--c-muted)" style="font-size:10px;font-weight:500" transform="rotate(-60 ${x + barW / 2} ${pad.t + ch + 12})">${years[i]}</text>`;
  }
  svg += '</svg>';
  return `<div class="card card-col-between">
    <div class="card-title">Yearly Productivity Pulse</div>
    <div class="chart-wrap">${svg}</div>
  </div>`;
}

function renderDayDonut() {
  const days = DATA.commits_by_weekday || [0,0,0,0,0,0,0];
  const total = days.reduce((a, b) => a + b, 0);
  const names = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const dayColors = ['#ef4444','#f59e0b','#eab308','#10b981','#3b82f6','#8b5cf6','#ec4899'];
  const slices = names.map((n, i) => ({ label: n, value: days[i], color: dayColors[i] }));
  const svg = donutSvg(slices, 160, 50);
  const legend = names.map((n, i) => {
    const pct = total > 0 ? (days[i] / total * 100).toFixed(1) : '0.0';
    return `<div class="legend-row">
      <div class="legend-label">
        <span class="legend-dot" style="background:${dayColors[i]}"></span>
        <span class="legend-text">${n}</span>
      </div>
      <div class="font-mono legend-value">
        <span>${fmt(days[i])} commits</span> <span class="legend-value-pct">${pct}%</span>
      </div>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Weekly Distribution</div>
    <div class="donut-layout">
      <div class="donut-chart">${svg}</div>
      <div class="donut-legend">${legend}</div>
    </div>
  </div>`;
}

function renderHourChart() {
  const hours = DATA.commits_by_hour || Array(24).fill(0);
  const maxH = Math.max(...hours, 1);
  const W = 500, H = 220;
  // Left padding scales with the tick label width so large counts don't collide with the axis title.
  const pad = { t: 16, r: 16, b: 40, l: 28 + fmt(maxH).length * 7 };
  const cw = W - pad.l - pad.r, ch = H - pad.t - pad.b;
  const step = cw / 24, barW = Math.max(2, Math.floor(step) - 2);
  let svg = `<svg width="100%" viewBox="0 0 ${W} ${H}" style="display:block">`;
  svg += `<text x="${12}" y="${pad.t + ch / 2}" text-anchor="middle" fill="var(--c-muted)" style="font-size:9px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em" transform="rotate(-90 12 ${pad.t + ch / 2})">Commits</text>`;
  svg += `<text x="${pad.l + cw / 2}" y="${H - 4}" text-anchor="middle" fill="var(--c-muted)" style="font-size:9px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em">Hour of Day</text>`;
  for (let i = 0; i <= 4; i++) {
    const y = pad.t + ch - (ch * i / 4);
    svg += `<line x1="${pad.l}" y1="${y}" x2="${W - pad.r}" y2="${y}" stroke="var(--c-grid)" stroke-width="1" ${i > 0 ? 'stroke-dasharray="4,4"' : ''}/>`;
    svg += `<text x="${pad.l - 8}" y="${y + 3}" text-anchor="end" fill="var(--c-muted)" style="font-size:10px;font-weight:500">${fmt(Math.round(maxH * i / 4))}</text>`;
  }
  for (let i = 0; i < 24; i++) {
    const x = pad.l + i * step + (step - barW) / 2;
    const h = (hours[i] / maxH) * ch;
    svg += `<rect x="${x}" y="${pad.t + ch - h}" width="${barW}" height="${h}" rx="1.5" fill="var(--c-accent)" opacity="0.9"><title>${String(i).padStart(2,'0')}:00 — ${fmt(hours[i])} commits</title></rect>`;
    if (i % 4 === 0) svg += `<text x="${x + barW / 2}" y="${pad.t + ch + 16}" text-anchor="middle" fill="var(--c-muted)" style="font-size:10px;font-weight:500">${String(i).padStart(2,'0')}</text>`;
  }
  svg += '</svg>';
  return `<div class="card card-col-between">
    <div class="card-title">Hourly Activity Profile</div>
    <div class="chart-wrap">${svg}</div>
  </div>`;
}

function renderContributors() {
  const authors = humanAuthors().slice(0, 10);
  const maxC = authors.length > 0 ? authors[0].commits : 1;
  const rows = authors.map((a, i) => {
    const pct = (a.commits / maxC * 100);
    return `<div class="list-row">
      <span class="list-rank">${i + 1}</span>
      <div class="list-main">
        <div class="list-head">
          <div class="list-head-label">
            <span class="list-title">${esc(a.name)}</span>
            <span class="list-title-sub">${esc(a.emails[0])}</span>
          </div>
          <span class="font-mono text-accent list-value">${fmt(a.commits)} commits</span>
        </div>
        <div class="bar-track"><div class="bar-fill" style="width:${pct}%"></div></div>
        <div class="font-mono list-sub"><span class="text-positive">+${fmt(a.additions)}</span> <span class="text-negative">-${fmt(a.deletions)}</span> · ${fmt(a.files_changed)} files</div>
      </div>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Top Contributors</div>
    <div class="scroll-list">${rows}</div>
  </div>`;
}

function renderFiles() {
  const files = DATA.top_files || [];
  const maxC = files.length > 0 ? files[0].change_count : 1;
  const rows = files.map((f, i) => {
    const pct = (f.change_count / maxC * 100);
    return `<div class="list-row">
      <span class="list-rank">${i + 1}</span>
      <div class="list-main">
        <div class="list-head">
          <span class="font-mono truncate-rtl">${esc(f.path)}</span>
          <span class="font-mono list-value">${fmt(f.change_count)} changes</span>
        </div>
        <div class="bar-track"><div class="bar-fill" style="width:${pct}%"></div></div>
        <div class="font-mono list-sub"><span class="text-positive">+${fmt(f.additions)}</span> <span class="text-negative">-${fmt(f.deletions)}</span></div>
      </div>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Churn Hotspots</div>
    <div class="scroll-list">${rows}</div>
  </div>`;
}

function renderImpact() {
  const sorted = DATA.top_impact_commits || [];
  const maxChurn = sorted.length > 0 ? sorted[0].additions + sorted[0].deletions : 1;
  const rows = sorted.map((c, i) => {
    const churn = c.additions + c.deletions;
    const pct = (churn / maxChurn * 100);
    return `<div class="list-row list-row--top">
      <span class="list-rank list-rank--top">${i + 1}</span>
      <div class="list-main">
        <div class="list-head">
          <div class="list-head-label">
            <span class="font-mono text-accent" style="font-weight:600">${c.id.slice(0, 7)}</span>
            <span class="list-msg">${esc(c.message)}</span>
          </div>
          <span class="font-mono list-value">${fmt(churn)} lines</span>
        </div>
        <div class="bar-track"><div class="bar-fill" style="width:${pct}%"></div></div>
        <div class="list-sub">
          ${esc(c.author_name)} · <span class="font-mono">${fmtDate(c.timestamp)}</span> · <span class="font-mono text-positive">+${fmt(c.additions)}</span> <span class="font-mono text-negative">-${fmt(c.deletions)}</span>
        </div>
      </div>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Highest Impact Commits</div>
    <div class="scroll-list">${rows}</div>
  </div>`;
}

function renderBusFactor() {
  const factors = DATA.bus_factors || [];
  if (!factors.length) return '';
  const sorted = [...factors].sort((a, b) => a.bus_factor - b.bus_factor);
  const legend = sorted.map((f) => {
    const badge = f.bus_factor <= 1 ? 'var(--c-negative)' : f.bus_factor <= 2 ? '#eab308' : 'var(--c-positive)';
    const top = f.top_authors.slice(0, 3).map(([n, p]) => `${esc(n)} ${p.toFixed(0)}%`).join(', ');
    return `<div class="bf-row">
      <div class="bf-main">
        <span class="bf-badge" style="background:${badge}">${f.bus_factor}</span>
        <div style="min-width:0">
          <div class="bf-dir">${esc(f.directory)}</div>
          <div class="bf-authors">${top}</div>
        </div>
      </div>
      <span class="font-mono bf-churn" title="${fmt(f.total_changes)} lines changed">${f.total_changes >= 1e6 ? (f.total_changes/1e6).toFixed(1)+'M' : f.total_changes >= 1e3 ? (f.total_changes/1e3).toFixed(1)+'K' : f.total_changes} churn</span>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Bus Factor Risk</div>
    <div class="scroll-list">${legend}</div>
  </div>`;
}

function renderCommitVelocity() {
  const dist = DATA.commit_size_distribution || [0,0,0,0,0];
  const score = DATA.cohesion_score || 0;
  const total = dist.reduce((a, b) => a + b, 0) || 1;
  const labels = ['Tiny (<10 lines)', 'Small (10-100 lines)', 'Medium (100-500 lines)', 'Large (500-1K lines)', 'Mega (>1K lines)'];
  const slColors = ['#22c55e','#3b82f6','#eab308','#f97316','#ef4444'];
  const slices = labels.map((label, i) => ({ label, value: dist[i], color: slColors[i] }));
  const svg = donutSvg(slices, 160, 50);
  const scoreColor = score >= 70 ? 'var(--c-positive)' : score >= 40 ? '#eab308' : 'var(--c-negative)';
  const legend = labels.map((label, i) => {
    const pct = (dist[i] / total * 100).toFixed(1);
    return `<div class="legend-row">
      <div class="legend-label">
        <span class="legend-dot" style="background:${slColors[i]}"></span>
        <span class="legend-text">${label}</span>
      </div>
      <div class="font-mono legend-value">
        <span>${fmt(dist[i])} commits</span> <span class="legend-value-pct">${pct}%</span>
      </div>
    </div>`;
  }).join('');
  return `<div class="card card-col">
    <div class="card-title">Commit Velocity</div>
    <div class="donut-layout">
      <div class="cohesion-col">
        ${svg}
        <div class="cohesion-score">
          <div class="cohesion-score-value" style="color:${scoreColor}">${score}%</div>
          <div class="cohesion-score-label">Cohesion Score</div>
        </div>
      </div>
      <div class="donut-legend">${legend}</div>
    </div>
  </div>`;
}

function renderCodeDecay() {
  const total = DATA.decay_total || 0;
  if (!total) return '';
  const buckets = DATA.decay_buckets || [0, 0, 0, 0, 0];
  const bucketDefs = [
    { label: '< 1 month', color: '#22c55e' },
    { label: '1-3 months', color: '#3b82f6' },
    { label: '3-6 months', color: '#eab308' },
    { label: '6-12 months', color: '#f97316' },
    { label: '> 1 year', color: '#ef4444' },
  ];
  const barW = 500, barH = 28;
  let barSvg = `<svg width="100%" viewBox="0 0 ${barW} ${barH}" class="decay-bar">`;
  let x = 0;
  bucketDefs.forEach((def, i) => {
    const w = total > 0 ? (buckets[i] / total) * barW : 0;
    if (w > 0) {
      barSvg += `<rect x="${x}" y="0" width="${w}" height="${barH}" fill="${def.color}" opacity="0.85"><title>${def.label}: ${buckets[i]} files (${(buckets[i]/total*100).toFixed(1)}%)</title></rect>`;
      x += w;
    }
  });
  barSvg += '</svg>';
  const legend = bucketDefs.map((def, i) => {
    const pct = total > 0 ? (buckets[i] / total * 100).toFixed(1) : '0.0';
    return `<div class="decay-legend-item">
      <span class="decay-legend-dot" style="background:${def.color}"></span>
      <span class="text-muted">${def.label}</span>
      <span class="font-mono" style="font-weight:600">${buckets[i]} files</span>
      <span class="text-muted decay-legend-pct">${pct}%</span>
    </div>`;
  }).join('');
  const stale = DATA.decay_stale || 0;
  const stalePct = total > 0 ? (stale / total * 100).toFixed(1) : '0.0';
  return `<div class="card">
    <div class="card-title">Code Decay</div>
    <div class="decay-note">Time since each file was last modified in a commit</div>
    ${barSvg}
    <div class="decay-legend">${legend}</div>
    <div class="decay-summary">
      Stale files (not modified in 6+ months): <span class="decay-stale" style="color:${stale/total > 0.5 ? 'var(--c-negative)' : 'var(--c-fg)'}">${stale} files (${stalePct}%)</span> of ${total} tracked files
    </div>
  </div>`;
}

function toggleTheme() {
  const html = document.documentElement;
  const target = html.classList.contains('dark') ? 'light' : 'dark';
  html.classList.remove('light', 'dark');
  html.classList.add(target);
  localStorage.setItem('gitat-theme', target);
}

(function() {
  const stored = localStorage.getItem('gitat-theme') || (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
  document.documentElement.classList.add(stored);
})();

document.getElementById('app').innerHTML = [
  // — Executive Summary
  `<div class="section">${renderVitalsAndHeatmap()}</div>`,
  section('section', [show('heatmap', renderHeatmap())]),

  // — Velocity & Engineering Rhythm
  sectionHeader('Velocity &amp; Engineering Rhythm', ['pulse', 'commit_velocity', 'weekly_distribution', 'hourly_activity']),
  section('grid-2 section', [
    show('pulse', renderPulse()),
    show('commit_velocity', renderCommitVelocity()),
    show('weekly_distribution', renderDayDonut()),
    show('hourly_activity', renderHourChart()),
  ]),

  // — Workforce & Community
  sectionHeader('Workforce &amp; Community', ['contributors', 'affiliations']),
  section('grid-2 section', [show('contributors', renderContributors()), show('affiliations', renderAffiliationsDonut())]),

  // — Codebase Architecture & Risk
  sectionHeader('Codebase Architecture &amp; Risk', ['churn_hotspots', 'bus_factor', 'impact_commits', 'code_decay']),
  section('grid-2 section', [show('churn_hotspots', renderFiles()), show('bus_factor', renderBusFactor())]),
  section('section', [show('impact_commits', renderImpact())]),
  section('section', [show('code_decay', renderCodeDecay())]),
].join('');
