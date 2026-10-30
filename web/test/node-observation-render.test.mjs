import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';

test('empty or fully omitted storage is not presented as an absent device; both locales identify it', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('..', import.meta.url)), configFile: false, appType: 'custom', server: { middlewareMode: true, hmr: false } });
  const originalDocument = globalThis.document;
  globalThis.document = { documentElement: { lang: '' }, title: '' };
  try {
    const { ObservationDetails } = await server.ssrLoadModule('/src/nodes.tsx');
    const { setLocale } = await server.ssrLoadModule('/src/i18n.ts');
    const node = { resources: { collected_ms: Date.now(), data: { filesystems: [] }, errors: { filesystems: 'observation size limit exceeded; some entries omitted', gpus: 'permission denied' }, truncated: true } };
    setLocale('zh-CN');
    let html = renderToStaticMarkup(createElement(ObservationDetails, { node, kind: 'resources' }));
    assert.match(html, /存储挂载点列表不完整/);
    assert.match(html, /分项采集错误/);
    assert.match(html, /permission denied/);
    assert.doesNotMatch(html, /未发现设备|observation size limit exceeded|数据未过期|数据已截断/);
    setLocale('en');
    html = renderToStaticMarkup(createElement(ObservationDetails, { node, kind: 'resources' }));
    assert.match(html, /Storage mounts list incomplete/);
    assert.match(html, /Some entries were omitted/);
    assert.doesNotMatch(html, /未|挂载|采集/);
    setLocale('zh-CN');
    const removed = { resources: { ...node.resources, data: {} } };
    html = renderToStaticMarkup(createElement(ObservationDetails, { node: removed, kind: 'resources' }));
    assert.match(html, /宿主机存储挂载点/);
    assert.match(html, /以下为已上报内容/);
  } finally {
    globalThis.document = originalDocument;
    await server.close();
  }
});
