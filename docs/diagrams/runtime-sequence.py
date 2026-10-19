"""Render the current two-page presentation sequence; run from any directory."""
from html import escape
from pathlib import Path
from PIL import ImageFont

OUT = Path(__file__).with_suffix('.svg')
W, H = 1840, 1840
BLUE, PURPLE, GREEN, ORANGE = '#2563eb', '#7c3aed', '#15803d', '#c2410c'
INK, MUTED = '#111827', '#475569'
parts = [f'''<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"
 data-generator="fireworks-tech-graph" data-style-id="1" data-diagram-type="sequence" data-quality-profile="showcase">
<title>Slint Bridge · 双 framebuffer 并行运行时序</title>
<desc>依据 src/runtime.rs、framebuffer.rs、presentation.rs、backends/mod.rs、backends/nx5/mod.rs、backends/x9/mod.rs、backends/f1/mod.rs。仅展示有脏区的成功提交路径；初始 A 为前台、B 为后台。渲染后及写页分段之间处理输入，写页期间冻结保留画布，整页完成后才提交。UI 预渲染 N+1 可与后台提交 N 的板级完成等待并行。完成成功后才允许复用旧前台页。板级回调与通用提交器属于同一个后台线程。</desc>
<style>text {{font-family:'Noto Sans CJK SC','PingFang SC','Microsoft YaHei',sans-serif;}}</style>
<defs>''']
for name, color in [('call', BLUE), ('async', PURPLE), ('return', GREEN), ('driver', ORANGE)]:
    parts.append(f'<marker id="arrow-{name}" markerWidth="9" markerHeight="7" refX="8" refY="3.5" orient="auto"><path d="M0,0 L9,3.5 L0,7 Z" fill="{color}"/></marker>')
parts.append('</defs>')
text_count = 0

def rect(x, y, w, h, fill, stroke='none', role='decoration', ident=None, radius=9, dash=None):
    attrs = f' id="{ident}"' if ident else ''
    if role in ('node', 'reserved'):
        attrs += f' data-graph-bounds="{x},{y},{x+w},{y+h}"'
    if role == 'node':
        attrs += f' data-node-id="{ident}"'
    if dash:
        attrs += f' stroke-dasharray="{dash}"'
    parts.append(f'<rect{attrs} data-graph-role="{role}" x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" fill="{fill}" stroke="{stroke}" stroke-width="1.6"/>')

def text(x, y, value, size=18, color=INK, weight=400, anchor='start', role='label', max_width=None, owner=None):
    global text_count
    text_count += 1
    font = ImageFont.truetype('/usr/share/fonts/opentype/noto/NotoSansCJK-' + ('Bold' if weight >= 600 else 'Regular') + '.ttc', size, index=2)
    width = font.getlength(value)
    if max_width is not None:
        assert width <= max_width, (value, width, max_width)
    left = x - (width / 2 if anchor == 'middle' else width if anchor == 'end' else 0)
    assert left >= 0 and left + width <= W, (value, left, width)
    attrs = f' data-owner="{owner}"' if owner else ''
    if role == 'label':
        attrs += f' data-graph-bounds="{left:.2f},{y-size},{left+width:.2f},{y+4}"'
    parts.append(f'<text id="text-{text_count}" data-graph-role="{role}"{attrs} x="{x}" y="{y}" text-anchor="{anchor}" font-size="{size}" font-weight="{weight}" fill="{color}">{escape(value)}</text>')

def line(x1, y1, x2, y2, color, dash=None, width=1.5):
    dashed = f' stroke-dasharray="{dash}"' if dash else ''
    parts.append(f'<line data-graph-role="decoration" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{width}"{dashed}/>')

def msg(ident, source, target, y, label, kind='call', size=18):
    colors = {'call': BLUE, 'async': PURPLE, 'return': GREEN, 'driver': ORANGE}
    x1, x2 = X[source], X[target]
    dashed = ' stroke-dasharray="8 5"' if kind in ('async', 'return') else ''
    parts.append(f'<line id="{ident}" data-graph-role="edge" data-edge-id="{ident}" data-source="{source}" data-target="{target}" x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="{colors[kind]}" stroke-width="2.3"{dashed} marker-end="url(#arrow-{kind})"/>')
    text((x1+x2)/2, y-15, label, size, colors[kind], 500, 'middle', max_width=abs(x2-x1)-12, owner=ident)

def note(ident, x, y, w, h, title, body, color=BLUE, fill='#eff6ff'):
    rect(x,y,w,h,fill,color,'reserved',ident)
    text(x+w/2,y+28,title,19,color,600,'middle',role='content',max_width=w-24)
    for i,s in enumerate(body):
        text(x+w/2,y+57+i*28,s,17,MUTED,400,'middle',role='content',max_width=w-24)

rect(0,0,W,H,'#ffffff',role='background',radius=0)
text(60,58,'Slint Bridge · 双 framebuffer 并行运行时序',32,weight=700)
text(60,92,'渲染下一帧与等待上一帧翻页重叠；页面复用由成功完成通知解锁。',20,color=MUTED)
rect(60,119,1160,37,'#eff6ff')
rect(1240,119,540,37,'#fff7ed')
text(80,145,'通用层：调度、画布、页面所有权、提交与完成通知',18,BLUE,600)
text(1260,145,'板级适配层及设备驱动',18,ORANGE,600)
rect(60,170,860,37,'#f8fafc')
rect(960,170,820,37,'#faf5ff')
text(80,196,'UI 线程执行的代码 / 管理的资源',17,MUTED,500)
text(980,196,'fb-present 的调用栈：同步调用板级回调，再进入驱动',17,PURPLE,500)

X = {'ui':200,'canvas':480,'pages':780,'worker':1080,'nx5':1370,'driver':1660}
headers = [
 ('ui','UI 渲染线程','runtime / framebuffer',BLUE,'#eff6ff'),
 ('canvas','保留画布 retained','TDE / GE：DMA 画布',BLUE,'#eff6ff'),
 ('pages','物理显示页 A / B','straight RGB / 板级 alpha',BLUE,'#eff6ff'),
 ('worker','后台提交线程','presentation.rs · fb-present',PURPLE,'#faf5ff'),
 ('nx5','板级完成回调','backends/{nx5,x9,f1}/',ORANGE,'#fff7ed'),
 ('driver','设备显示驱动','NX5 / X9 / F1 板级约定',ORANGE,'#fff7ed'),
]
for ident, title, sub, color, fill in headers:
    cx = X[ident]
    rect(cx-120,230,240,72,fill,color,'node',ident)
    text(cx,259,title,20,color,600,'middle',role='content',max_width=218)
    text(cx,284,sub,15,MUTED,400,'middle',role='content',max_width=218)
    line(cx,367,cx,1460,'#cbd5e1','5 7')
line(1228,215,1228,1475,'#fdba74','6 7')
rect(60,320,1720,43,'#f8fafc')
text(80,349,'示例起点：A（yoffset=0）为前台；B（yoffset=H）可写；H 为设备高度。时间从上往下，图中高度不代表实际耗时。',18,MUTED)
text(25,390,'t ↓',16,MUTED)
line(38,410,38,1440,'#94a3b8')

# Thin activation bars are UML execution notation, not separate components.
for ident,y,h,color,fill in [
 ('ui',402,171,BLUE,'#dbeafe'), ('canvas',410,40,BLUE,'#dbeafe'),
 ('ui',780,65,BLUE,'#dbeafe'), ('canvas',790,55,BLUE,'#dbeafe'),
 ('ui',1218,215,BLUE,'#dbeafe'), ('worker',565,657,PURPLE,'#ede9fe'),
 ('nx5',625,540,ORANGE,'#ffedd5'), ('driver',680,330,ORANGE,'#ffedd5'),
 ('driver',1065,45,ORANGE,'#ffedd5')]:
    rect(X[ident]-6,y,12,h,fill,color,radius=2)

msg('render-n','ui','canvas',410,'① render(N) → 画布',size=18)
text(498,440,'CPU 绘制 / 同步 TDE 或 GE',17,MUTED)
text(212,453,'渲染后先处理输入',17,BLUE)
text(212,517,'首次整页；随后合并两帧脏区；编码 alpha；源画布不变',16,MUTED)
msg('copy-b','ui','pages',495,'② 分段反预乘 / 写 B；段间处理输入')
msg('submit-b','ui','worker',565,'③ submit(H)：入队即返回；pending = true','async')
msg('contract-b','worker','nx5',625,'④ complete(fd, H, layout)',size=17)
msg('pan-b','nx5','driver',680,'⑤ PAN；X9 flip / F1 VSYNC','driver',18)

rect(70,732,1710,234,'#f0fdf4','#86efac',radius=10)
# Restore lifeline/activation fragments on the tinted parallel frame.
for ident,cx in X.items():
    line(cx,733,cx,965,'#cbd5e1','5 7')
line(1228,733,1228,965,'#fdba74','6 7')
for ident,y,h,color,fill in [
 ('ui',780,65,BLUE,'#dbeafe'),('canvas',790,55,BLUE,'#dbeafe'),
 ('worker',733,232,PURPLE,'#ede9fe'),('nx5',733,232,ORANGE,'#ffedd5'),('driver',733,232,ORANGE,'#ffedd5')]:
    rect(X[ident]-6,y,12,h,fill,color,radius=2)
rect(82,774,73,32,'#dcfce7','#86efac')
text(118,796,'PAR',18,GREEN,700,'middle',role='content')
msg('render-next','ui','canvas',790,'render(N+1) → 画布',size=18)
note('ui-work',96,857,498,77,'最多预渲染一帧，然后暂停后续绘制',['任务每批约 2 ms / 最多 128 个；继续处理输入'],BLUE,'#eff6ff')
note('frozen-pages',638,766,284,167,'pending 期间两页均不可写',['A：旧前台，等待释放','B：提交中，禁止修改','画布最多再渲染一帧'],GREEN,'#ffffff')
note('blocked-worker',990,801,735,106,'后台线程等待板级完成确认',['等待驱动完成；通用层没有发起第二次页面提交'],ORANGE,'#fff7ed')

msg('pan-done','driver','nx5',1010,'⑥ 板级完成确认','return',18)
msg('get-offset','nx5','driver',1065,'GET + 校验布局 / 页号','driver',18)
msg('checked-offset','driver','nx5',1110,'一致：仅作错误检查','return',18)
msg('contract-done','nx5','worker',1165,'⑦ Ok：旧前台可复用','return',18)
msg('wake-ui','worker','ui',1220,'⑧ completion.send(Ok) + eventfd 唤醒','async')
note('poll-success',94,1262,500,58,'poll_present() 成功 → 更新 front = B',[],BLUE,'#eff6ff')
note('released-page',642,1262,276,58,'B 为前台；A 可写',[],GREEN,'#f0fdf4')
msg('copy-a','ui','pages',1360,'⑨ 分段写 A；段间处理输入，画布保持不变')
msg('submit-a','ui','worker',1425,'⑩ submit(0)：提交 N+1；随后可预渲染 N+2','async')

rect(60,1490,1720,44,'#f0fdf4')
text(80,1520,'循环：A 提交后两页再次冻结；UI 在保留画布渲染 N+2，后台等待 A 的翻页完成。',19,GREEN,500)
note('capacity',60,1565,540,143,'并行与容量',['最多 1 次提交进行中 + 1 帧等待提交','画布冻结至整页提交；不是第三张物理页','约 1 ms 是写页软预算；Slint 渲染不抢占'],BLUE,'#eff6ff')
note('generic-contract',640,1565,540,143,'通用层负责',['事件 / 帧期限、脏区复制、前后台角色','提交线程、完成队列、eventfd 与错误传播','通过私有 PresentationContract 调用板级实现'],PURPLE,'#faf5ff')
note('board-contract',1220,1565,560,143,'板级适配负责',['NX5：阻塞 PAN 成功返回','X9：PAN 后等待匹配 flip 事件','F1：参考 PAN + VSYNC；退休未独立证明'],ORANGE,'#fff7ed')

# Legend outside the message corridors.
for x,name,color,label,dash in [(70,'call',BLUE,'同步调用 / UI 复制',False),(440,'async',PURPLE,'跨线程请求 / 完成通知',True),(900,'return',GREEN,'成功返回',True),(1215,'driver',ORANGE,'板级驱动调用',False)]:
    dash_attr = ' stroke-dasharray="7 4"' if dash else ''
    parts.append(f'<line data-graph-role="decoration" x1="{x}" y1="1748" x2="{x+43}" y2="1748" stroke="{color}" stroke-width="2.3"{dash_attr} marker-end="url(#arrow-{name})"/>')
    text(x+56,1754,label,17,MUTED)
text(60,1794,'图示有脏区的成功路径。空脏区不提交；提交失败后禁止继续写页；写页期间保留画布冻结。',17,MUTED)
text(60,1823,'退出事件循环：等待提交并丢弃未提交帧；销毁 Framebuffer：join 后释放资源，未知 DMA 时保留隔离。',17,MUTED)
parts.append('</svg>')
OUT.write_text('\n'.join(parts)+'\n',encoding='utf-8')
print(f'{OUT}: {W} × {H}; all {text_count} text runs fit their declared width')
