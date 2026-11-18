import { ImportPanel, useImporter } from './Papers';
import { pickBib } from '../lib/dialogs';
import { Button, Card, PageHeader } from '../lib/ui';

export default function Zotero() {
  const imp = useImporter();
  return (
    <div>
      <PageHeader title="Zotero 가져오기" subtitle="Zotero에서 내보낸 BibTeX(.bib) 또는 RIS(.ris) 파일로 서지정보를 가져옵니다." actions={<Button kind="primary" onClick={async () => imp.start(await pickBib())}>서지 파일 선택</Button>} />
      <ImportPanel imp={imp} />
      <div className="grid gap-5 lg:grid-cols-2">
        <Card title="Zotero에서 내보내는 방법">
          <ol className="list-decimal space-y-1.5 pl-5 text-sm">
            <li>Zotero에서 내보낼 컬렉션을 오른쪽 클릭 → <b>컬렉션 내보내기…</b> (전체는 <b>파일 → 라이브러리 내보내기…</b>)</li>
            <li>형식에서 <b>BibTeX</b> 또는 <b>RIS</b>를 고릅니다.</li>
            <li>PDF도 함께 등록하려면 <b>파일 내보내기(Export Files)</b>를 체크합니다. 내보낸 폴더 안의 PDF 경로가 서지 파일에 기록됩니다.</li>
            <li>만들어진 .bib / .ris 파일을 이 화면에서 선택합니다.</li>
          </ol>
        </Card>
        <Card title="가져올 때 일어나는 일">
          <ul className="list-disc space-y-1.5 pl-5 text-sm">
            <li>DOI 또는 제목이 같은 논문이 이미 있으면 새로 만들지 않고 <b>비어 있거나 자동 추정된 서지정보만</b> 채웁니다 (중복 검사·서지정보 연결). 교수님이 직접 입력한 값은 덮어쓰지 않습니다.</li>
            <li>서지 파일에 PDF 경로가 있고 파일이 실제로 있으면 PDF도 함께 등록합니다.</li>
            <li>PDF가 없으면 “서지정보만” 항목으로 등록합니다. 나중에 PDF를 등록하면 본문 검색이 가능합니다.</li>
            <li>Zotero의 내부 데이터베이스는 읽거나 수정하지 않습니다. 향후 Zotero 공식 Web API 연동을 지원할 수 있도록 설계되어 있습니다.</li>
          </ul>
        </Card>
      </div>
    </div>
  );
}
