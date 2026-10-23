# 직접 CLI 우선 개선 설계

- 작성일/방향 변경: 2026-10-22
- 최신 사용자 결정: **console 개발을 중단하고 직접 CLI 개선을 우선한다.** 사용자가 현재 console을 확인한 뒤 직관성이 부족하다고 평가했다.
- 상태: 이 문서는 새 우선순위와 목표 계약이다. 기존 UX-01~03 구현·검증 이력을 보존하며, 2026-10-23 CLI-01/02를 구현·검증했다. [독립 관리 CLI 설계](./2026-10-23-independent-management-cli.md)에 따라 기존 RC.3 실행기 위임과 이름 기반 운용을 제공한다. CLI-03의 순차 문답과 조회·수정을 구현·검증했다. [CLI-03 검증 기록](../results/2026-10-23-cli03-sequential-preparation.md)을 보존한다. CLI-04의 준비 결과에서 설치·등록·이름 연결과 이름 기반 초기화도 구현·검증했다. [CLI-04 검증 기록](../results/2026-10-23-cli04-apply-and-lifecycle.md)을 따른다. 이후 CLI-05가 남는다.
- 연결: [현재 구현 상태](../docs/implementation-status.md), [작업 순서](./2026-10-22-integrated-operations-ux-implementation-plan.md).

## 1. 바꾸는 방향

작업 하나를 명령 하나로 실행하고 결과와 다음 필요한 명령을 보여준다. 노드를 선택하려고 console을 먼저 열거나 메뉴 계층을 왕복하지 않는다. 입력이 많은 초기 준비는 필요한 값만 순서대로 질문하고 최종 검토·명시 저장 후 종료한다. 일상 운용은 문답 루프에 들어가지 않는다.

CLI-03에서 직접 `setup new/resume`의 7개 번호 메뉴를 순차 질문으로 교체했다. Preparation의 저장·재개·검증 API와 기존 세션은 재사용한다. console의 기존 메뉴는 보존하고 직접 CLI 진입점만 순차 문답을 사용한다. 검증 결과와 사용자 과업 검증은 구현 상태와 별도로 기록한다.

console 홈·메뉴·화면 시안의 추가 개발과 실제 서비스 연결을 중단한다. 기존 코드·시험 기록은 보존하며 이번 결정으로 명령을 삭제하거나 저장 자료를 지우지 않는다. 향후 재개는 별도 사용자 결정으로 정한다. CLI 완료 조건에 console 기능 동등성이나 S01~S11 화면 완성을 넣지 않는다.

## 2. 목표 명령

CLI-01 목록/이름 연결과 CLI-02 이름 기반 운용을 구현했다. CLI-03의 순차 준비·조회·수정도 구현했다. CLI-04도 **구현·검증 완료**이며 CLI-05는 **계획**이다. 명령 사용법은 [CLI 가이드](../docs/cli.md), 검증 상태는 [구현 현황](../docs/implementation-status.md)을 따른다.

| 과업 | 목표 문법 | 의미/단계 |
| --- | --- | --- |
| 내 노드 목록 | `bxdl nodes [--home <root>]` | CLI-01 구현. 저장 목록 조회, live 상태는 미조회 |
| 기존 노드에 이름 붙이기 | `bxdl nodes add local --instance <control> [--home <root>]` | CLI-01 구현. 지원되는 기존 등록을 확인하고 탐색 목록에 연결. 설치/엔진 재등록 아님 |
| 상태·시작·중지 | `bxdl status local`, `bxdl start local`, `bxdl stop local` | CLI-02 구현. 명시한 한 노드의 검증된 실행기/기존 API 호출 |
| 기록·지원 자료 | `bxdl logs local`, `bxdl diagnose local --output report.json` | CLI-02 구현. 기존 정제 범위와 partial 계약 유지 |
| 새 설치 자료 준비 | `bxdl setup new local [--home <root>] [--from <instance.json>]` | CLI-03. 기존 진입 명령 유지, 번호 메뉴를 순차 문답으로 교체 |
| 준비 이어가기 | `bxdl setup resume local [--home <root>]` | CLI-03. 현재 저장 단계와 다음 필요한 입력부터 계속 |
| 준비 목록·내용 확인 | `bxdl setup list`, `bxdl setup show local` | CLI-03의 비TTY/JSON 무쓰기 조회. CLI-04에서 준비 기록과 설치 실행 기록을 분리 표시하고 현재 runtime은 status로 확인 |
| 제품 한 항목 수정 | `bxdl setup edit local --field httpPort` | CLI-03. 지정 필드만 질문하고 변경 영향·다음 명령 표시 후 종료 |
| 설치·등록 실행 | `bxdl setup apply local [--home <root>]` | CLI-04. TTY y/yes 승인 뒤 설치·cold 검사·등록·이름 연결. 완료 재호출은 검증과 이름 복구만 수행 |
| 초기화·조건부 재개 | `bxdl init local --confirm-initialize`, `bxdl resume-init local --confirm-resume` | CLI-04. 이름/--home 또는 기존 --instance. 기존 확인·재개 조건 유지. 설치나 start와 일괄 승인하지 않음 |
| 자동완성 | `bxdl completion <shell>` | CLI-05. 검증한 셸의 명령·옵션·등록 이름 후보 제공 |

이름을 받는 명령에는 동일한 `--home <root>` 규칙을 적용한다. Mac 기본 관리 위치와 기존 명령의 `--instance <path>`를 유지한다. `install <archive> --destination ...`의 기존 파일 설치 의미도 바꾸지 않는다. `restart/up/down` 등 중복 동사를 추가하지 않는다.

`nodes add`를 통해 console 없이 기존 노드를 처음 연결할 수 있어야 한다. 기존 카탈로그 entry ID를 알거나 console에서 먼저 저장해두는 것을 선행 조건으로 삼지 않는다. 이미 연결된 동일 대상은 기존 ID를 재사용할 수 있지만, 다른 대상이 사용하는 이름은 거부한다. 이름 변경/목록 제외 같은 추가 명령은 기본 경로 인수 뒤 필요성을 확인하여 범위를 정한다.

## 3. 이름과 대상 해석

- 표시 이름은 사람용 설명이며 중복·한글을 허용한다. 명령의 실행 대상으로 해석하지 않는다.
- 카탈로그의 불변 opaque ID는 실제 항목을 식별한다. 이름 변경과 무관하게 유지한다.
- 노드 명령 이름과 준비 세션명은 각각 관리 루트 내 고유 이름이다. `^[a-z][a-z0-9-]{0,31}$`를 적용하며 대문자 변환·공백 제거·경로 추정을 하지 않는다.
- 준비만으로 세션명이나 engine instanceId를 노드 별칭으로 채택하지 않는다. `setup apply`의 검토·승인 범위에 같은 세션 이름의 노드 연결을 포함하고 충돌과 등록 대상 identity를 확인한다. 이미 같은 이름/대상을 연결했다면 재저장하지 않는다.
- 위치 인자 이름과 기존 `--instance`는 둘 중 하나만 받는다. 대상 누락·미지 이름·손상·대상 교체는 usage 또는 검증 오류이며 마지막 노드/유일 노드로 추정하지 않는다.

이름 연결은 기존 등록/준비 파일을 읽고 탐색 정보를 저장하는 별도 작업이다. 데이터 채택·설치·초기화·서비스 변경을 하지 않는다. 기존 카탈로그의 표시명을 임의로 실행 이름으로 전환하지 않는다. 새 별칭 저장 계약과 기존 record 읽기 호환, 동시 갱신·고유성 검사, 실패 시 기존 정보 보존을 CLI-01에서 구현했다. V1 읽기는 무변환이고 명시 add만 catalog V2를 기록한다. `nodes add` 자체가 목록 저장 요청이며 문답은 없다. 같은 이름/대상 재요청은 무쓰기이며 구형 CLI의 V2 목록 읽기는 지원하지 않는다.

운용 이름을 해석한 뒤 dispatch 직전에 목록 revision과 control root/binding identity를 다시 검사한다. 기존 실행기의 lock·파일 pin·CLI 일치 검사·UNKNOWN 처리도 유지한다. 변경된 목록/root/binding을 발견하면 거절한다. 구형 실행기는 expected-attempt 원자적 승인 인자가 없으므로 직전 재검사와 child 잠금 사이의 한계는 [실행기 분리 설계](./2026-10-23-independent-management-cli.md)를 따른다. 셸 문자열·외부 CLI 실행 경로를 별칭에 저장하지 않는다.

## 4. 한 명령의 출력과 다음 행동

사람용 기본 출력은 대상 이름, 관측 결과, 필요한 이유, 다음 명령을 우선한다. 상세 경로·공급 hash·내부 단계 전체를 정상 결과마다 나열하지 않는다. credential 원문·경로·원문 엔진 출력은 기존 정제 정책을 유지한다.

예시 출력은 시안이며 실제 상태 이름/표현은 기존 공개 DTO에서 투영한다.

```text
local  시작 중
로컬 준비 상태를 아직 확인하지 못했습니다.
다음 확인: bxdl status local --wait
```

`nodes`는 저장 시각과 미조회 상태를 표시하고 모든 노드에 HTTP/status를 호출하지 않는다. `status`만 선택한 한 노드를 관측한다. `logs`·`diagnose`의 저장 기록과 현재 runtime 관측을 혼동하지 않는다. READY가 합의/거래 확정 전체를 뜻하지 않으며 대기 만료를 종료·실패 확정으로 바꾸지 않는다.

명령별 도움말은 가장 흔한 예제, 필수 입력, 결과 의미, 다음 명령을 보여준다. 최상위 도움말은 일상 운용·설치 준비·제작자 명령을 구분하고 console을 기본 시작점으로 권하지 않는다. 도움말을 조회해 설치·저장·실행하지 않는다.

## 5. 설치 준비를 순차 흐름으로 교체

`setup new`의 순차 흐름은 다음과 같다.

1. 세션 이름·새 관리 위치를 확인하고 준비 기록 생성 승인.
2. 공급 archive·독립 신뢰키 또는 명시 개발 정책·engine lock 선택.
3. 공통 chain/network에서 자기 노드 선택 또는 제공된 native 선택.
4. 기존 제품 설정의 값을 재사용하고 아직 필요한 제품·credential 참조·정책만 질문.
5. 공개 설정·선택 자료·출력 계획과 남은 검사를 검토.
6. 명시 승인 뒤 제품/native revision 준비. 현재 결과와 다음 한 명령을 보여주고 종료.

`:help`, 앞 질문으로 이동하는 `:back`, 취소/EOF와 체크포인트를 유지한다. 검토의 `edit <필드>`는 제품 또는 공급·경로·정책 한 항목을 묻고 다시 검토한다. `details`로 공개 자료·출력 경로를 확인한다. `y/yes`는 준비 승인, Enter/n/no는 저장한 답을 보존하고 종료다. 사용자가 공급 자료→제품→network 순서를 번호 메뉴로 조합하지 않아도 된다. 재개는 부족한 입력부터 시작하며 이미 준비한 결과는 현재 자료와 대조해 검토한다. 유효한 기존 결과에 변경이 없으면 승인해도 새 revision을 만들지 않는다. 자동 설치·init·start는 하지 않는다.

`setup list`는 관리 위치의 준비 세션을 나열하고 외부 자료를 재검증하지 않는다. 없는 관리 위치에 빈 목록을 반환하면서 폴더를 생성하지 않는다. `setup show`는 기록이 있는 준비 결과를 현재 참조 자료와 재대조한다. `PREPARED_RECORDED` 기록과 현재 검사 상태 `NOT_PREPARED/NOT_CHECKED/VERIFIED/CHANGED`를 분리하며 live 실행 상태를 조회하지 않는다. 읽을 수 있는 미완성 초안 조회는 성공이고, 목록의 일부 세션을 확인할 수 없거나 show의 현재 자료 검증이 실패하면 INCOMPLETE/exit 5다. 세션/잠금/설정 파일을 쓰지 않고 작업자의 배타적 준비 잠금도 획득하지 않는다.

`setup edit <이름> --field <제품필드>`는 한 제품 항목만 수정하고 다른 답을 보존한다. 실제 변경은 이전 검토를 무효화하며 기존 immutable 출력은 덮어쓰지 않는다. 변경 없는 입력은 checkpoint를 추가하지 않는다. 이 명령은 자동으로 새 설정을 준비하지 않으며 공급 자료/경로/정책 수정은 resume 검토에서 수행한다. 출력 실패·자료 변경·UNKNOWN을 성공으로 채택하거나 자동 재시도하지 않는다.

기존 비대화형 setup과 `--json` 계약은 유지한다. list/show는 버전형 정제 요약을 기존 JSON envelope 안에 반환하고 원본 답변·credential 참조 경로를 제외한다. new/resume/edit은 비TTY와 `--json`을 입력·쓰기 전에 거절한다. secret 원문을 인자에 받거나 `--json`을 대화형 승인으로 취급하지 않는다.

## 6. 실제 설치 실행과 호환

`setup apply <이름> [--home <root>]`는 준비를 실제 설치로 연결한다. TTY에서 준비 revision·선택 자료·출력 위치·이름 연결 범위를 검토하고 `y/yes`로 최종 승인한다. Enter/n/no는 실행을 거절하고 취소/EOF는 기존 자료를 보존한다. `--json`/비TTY는 입력·쓰기 전에 거절한다. 준비 재개나 show는 실행 승인이 아니다.

준비는 `sessions/<이름>`, 실행은 `operations/<이름>`에 별도로 보존한다. 준비 저장 형식은 변환하지 않는다. 실행 source는 준비 컨테이너 identity·검토 hash·제품/native revision/hash·공급 identity·대상 위치를 결속한다. 승인 전후에 자료·대상·카탈로그를 재대조하며 실제 파일 설치·cold 검사·control 등록은 기존 workflow의 durable intent·소유 증거·잠금으로 보호한다. DB init과 LaunchAgent start는 포함하지 않는다.

완료한 `apply`를 다시 실행하면 기존 설치·등록의 완료 근거와 연결한 준비 revision을 검사하고 필요한 이름 연결만 복구한다. 이미 완료한 설치를 반복하지 않는다. 같은 이름이 다른 노드에 연결되어 있으면 덮어쓰지 않으며 설치 완료와 이름 연결 실패를 별도 결과로 보존한다. 기존 control 경로의 조회·운용 안내를 제공한다. 준비의 `SOURCE_CHANGED`, 미확정 intent·`UNKNOWN`, 소유 근거 없는 부분 출력은 자동 재설치·재등록하거나 성공으로 채택하지 않는다.

완료 V2 등록은 control에 보관한 공급 자료로 검증하므로 원래 archive/key/lock의 부재만으로 재설치를 요구하지 않는다. 준비 revision·제품/native·chain/credential 참조와 설치/control identity는 계속 확인한다. 준비를 수정해 검토를 무효화하거나 revision을 바꾸면 기존 실행과 일치하지 않는 것으로 거절한다. 이를 migration이나 자동 복구로 취급하지 않는다.

`setup list/show`는 준비 기록과 별도의 설치 실행 기록을 읽기 전용으로 표시한다. sessions/operations 이름을 합쳐 조회하며 준비 세션의 삭제·손상으로 별도 실행 기록이 숨겨지지 않도록 준비 읽기 실패와 실행 요약을 분리한다. 준비의 `PREPARED_RECORDED`·현재 자료 대조와 설치 완료를 합치지 않는다. 현재 서비스 상태는 `status`로 관측한다. 설치 결과가 존재해도 새 조회만으로 파일 설치·등록·이름 연결을 재시도하지 않는다.

이름 기반 `init`과 `resume-init`은 기존 확인 플래그·timeout·UNKNOWN/exit 계약을 유지한다. 이름과 `--instance`는 배타적이며 `--home`은 이름에만 적용한다. 목록/root/binding을 해석하고 native 호출 또는 고정 RC.3 executor dispatch 직전에 다시 검사한다. 확인 플래그 누락·미지 이름·대상 교체에서 다른 노드를 추정해 선택하지 않는다.

관리 CLI와 package CLI의 bytes가 달라도 지정한 RC.3 profile은 검증한 패키지 실행기로 위임한다. 기존 archive·실행기 bytes와 서명/inventory는 유지하며 관리 CLI×runtime 조합을 검증한다. 미지원 package의 native 경로는 기존 exact CLI 조건을 유지한다. engine/JRE pin을 UX 편의 때문에 변경하지 않으며 NIGO에 새 console/registry 구현을 요구하지 않는다.

## 7. 완료 기준

메뉴 화면 수보다 다음 과업이 console 없이 되는지 검증한다.

- 새 터미널에서 기존 control에 이름을 붙이고 목록→status→logs를 실행한다.
- 같은 이름으로 명시 stop을 확인하고 별도 start를 실행한다. 실제 서비스 인수는 동일한 기존 RC.3 package와 새 관리 CLI의 조합으로 수행한다. runtime 변경 시에만 새 package를 인수한다.
- 처음 설치 준비에서 누락 값만 답하고, 잘못된 값 하나를 수정하고, 다음 날 이름으로 재개한다.
- 오류 메시지에서 다음 명령을 찾는다. 비TTY 명령은 무입력 동작 또는 명확한 usage 오류로 종료한다.
- 기존 경로 명령·JSON·종료 코드·저장 포맷과 정상/UNKNOWN 안전 조건이 유지된다.

코드 검사, 실제 Mac PTY, 관리 CLI/runtime 조합 인수, 작성자가 아닌 사람의 과업을 따로 기록한다. 과거 UX-03 Rust464/Python50 PASS는 기존 구현의 이력이며 이번 순차 CLI의 검증 근거로 재사용하지 않는다. 안전 제거 REM-01 등 G1-M의 별도 필수 과제도 유지한다.
