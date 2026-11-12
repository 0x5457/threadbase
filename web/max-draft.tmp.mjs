import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
const threadId = '01M75KYCF0JDZ5E11C9BAZ44BZ';
const doc = new Y.Doc();
const provider = new WebsocketProvider('ws://127.0.0.1:7800/ws/drafts', threadId, doc);
const blocks = doc.getArray('blocks');
provider.on('sync', () => {
  const block = new Y.Map();
  const text = new Y.Text();
  text.insert(0, 'Can we keep the 30s default? — Max');
  const id = crypto.randomUUID();
  doc.transact(() => {
    block.set('id', id);
    block.set('author', 'max-test');
    block.set('createdAt', Date.now());
    block.set('text', text);
    blocks.push([block]);
  });
  provider.awareness.setLocalStateField('user', { id: 'max-test', displayName: 'Max Kim', color: '#5c78e2' });
  provider.awareness.setLocalStateField('blockId', id);
  console.log('max draft inserted');
});
setTimeout(() => { provider.destroy(); doc.destroy(); process.exit(0); }, 12000);
