# 통합 설치·운용 UX 단계별 구현 계획

- 작성일: 2026-10-22 / 상태: **UX-01~07 모두 PLAN. 이 문서 작성은 구현 완료가 아니다.**
- 기준: [통합 설계](./2026-10-22-integrated-operations-ux.md), [화면 명세 S01~S11](./2026-10-22-integrated-operations-ux-screens.md), [화면 시안](./prototypes/integrated-operations-ux.html).
- 기존 M/BX 작업 ID와 G1/G2를 대체하지 않는다. UX ID는 이번 개선을 나눠 구현·검토하기 위한 하위 추적이다.
- 실제 코드 기준은 PR #15의 `870ca72`. 아래 새 `src/operator/`와 테스트 파일명은 제안이다.

## 1. 순서와 단계별 결과

| 단계 | 사용자가 얻는 변화 | 화면 | 기존 작업 | 선행 | 상태 |
| --- | --- | --- | --- | --- | --- |
| UX-01 | 도움말·뒤로·취소·항목 수정·상태 설명이 일관됨 | 공통, S03/S10 | BX-010/030/032 | 없음 | PLAN |
| UX-02 | 홈과 내 노드/설치 목록에서 대상을 찾음 | S01/S06/S07 | BX-010/030 | UX-01 상태 모델 | PLAN |
| UX-03 | 자료 확인부터 제품·network 설정을 연결 | S02/S03/S04 | BX-020/021/023/030 | UX-01; 관리 루트는 UX-02 | PLAN |
| UX-04 | 하나의 설치 흐름에서 명시 실행·재개 | S04/S05/S06 | BX-022/030 | UX-02/03 | PLAN |
| UX-05 | 노드 선택 후 일상 상태·중지·시작 | S07/S08/S09 | BX-031/032 | UX-01/02 | PLAN |
| UX-06 | 문제별 수정·관측·지원 보고서 연결 | S10/S11 | BX-023/033/042 | UX-04/05 | PLAN |
| UX-07 | 새 package와 처음 사용자 과업으로 검증 | 전체 | BX-040~044 | UX-01~06 | PLAN |

기본 착수 순서는 UX-01 → UX-02 → UX-03 → UX-04 → UX-05 → UX-06 → UX-07이다. UX-03의 순수 자료 모델과 UX-02 카탈로그는 공통 계약을 고정한 뒤 병행할 수 있다. UX-05는 UX-02 뒤 기존 등록의 지원되는 읽기 경로나 같은 새 UX CLI를 기존 저수준 API로 등록한 fixture를 대상으로 병행할 수 있다. 실제 start/stop 인수는 CLI가 일치하는 새 package로 UX-04와 합쳐 수행한다. 기본 오류 분기와 동작 차단은 각 단계부터 제공하고 UX-06에서 지원 흐름을 완성한다.

한 단계는 검토 가능한 1개 PR을 기본으로 하되, 공통 모델 추출과 동작 추가가 크면 나눈다. 공개 계약 변경·consumer·fixture는 함께 검증한다. 문서는 각 PR에서 동기화하며 마지막에 몰아서 갱신하지 않는다. 구현 기간·출시 날짜를 검증 없이 고정하지 않는다.

## 2. 공통 구조

```text
src/operator/                 # 신규 제안; 실제 파일 분리는 PR에서 확정
  mod.rs                      # console 진입과 화면 전환
  prompt.rs                   # 메타 입력·확인·항목 편집
  view.rs                     # 사람이 읽는 상태와 허용/권장 행동
  catalog.rs                  # private 탐색용 목록, authoritative 아님
  home.rs                     # 관리 루트·안전한 새 부모 준비
  install_flow.rs             # 기존 초안/network/workflow의 연결
  node.rs                     # 반복 운용; 설치 journal과 분리
  support.rs                  # 기존 정제 diagnostics 연결
```

기존 JSON 출력 DTO를 UI 편의 때문에 바꾸지 않는다. `view`는 공개 상태 투영을 받고 설명·권장 행동을 반환한다. path/credential을 포함한 원문 오류나 전체 engine payload를 화면에 덤프하지 않는다. 실효 가능 여부는 실행 직전에 기존 adapter가 다시 판정한다.

카탈로그와 새 console 조정 기록은 별도 버전형 저장소다. 기존 Draft/Network/Workflow/binding의 인코딩과 replay를 암묵적으로 바꾸지 않는다. 새 조정 기록은 하위 세션의 경로·종류·revision·소유 identity를 참조하고 작업 결과를 임의로 완료 처리하지 않는다. 중단 뒤 조정 기록과 실제 하위 결과의 불일치는 reconciliation 대상으로 보존한다.

## UX-01. 공통 문답·도움말·상태 표현

**산출물:** 공통 입력/확인/화면 투영, 명령별 도움말, 현 가이드의 RC.3/V1/V2 설명 정리. 이 PR에서 통합 설치 완료를 주장하지 않는다.

- 접점: `src/cli.rs`, `src/setup/{wizard,network_wizard,install_wizard}.rs`, `src/setup/workflow.rs`; 새 `operator/{prompt,view}.rs`.
- 모든 공급/제품/네트워크 입력에서 `:help`, `:back`, `:cancel`, EOF를 동일하게 해석한다. 예약 문자가 실제 파일명인 경우 명시 경로 표기법을 안내한다.
- `start --help`, `setup --install --help` 등 해당 명령의 문맥별 안내를 제공한다. 기존 top-level help 명령과 자동화 결과 계약은 유지한다.
- 항목별 편집과 공개 값 검토를 추가한다. 자격 자료는 종류·선택·검사 결과를 보여주며 비밀번호 원문을 출력하지 않는다.
- 입력 오류, 알려진 cold INCOMPLETE, 시작 대기 만료, 실제 실패, 상태 불명, 정상 종료 확인 필요를 구별하는 투영/행동 표를 만든다.
- `docs/{setup,network,instance,cli,install}.md`의 현재 절차를 정리한다. 과거 results와 공급 snapshot의 NOT_RUN/INCOMPLETE는 바꾸지 않는다.

**검증/완료 기준:** `tests/{cli,setup_cli,setup_network_cli,setup_install,setup_safety}.rs`에 의미 있는 회귀를 추가한다. `:help`가 path로 저장되지 않음, 기존 답 보존, 하나의 항목 수정, 취소·빈 Enter로 mutation 없음, nonTTY 입력 미소비, 기존 JSON/exit·stored schema 불변을 확인한다. 상태 표시 테스트는 문자열 복제보다 원 근거→허용 행동의 관계를 검사한다.

## UX-02. 홈·카탈로그·기본 관리 위치

**산출물:** TTY용 `bxdl console`, 홈, 설치 목록, 노드 목록, 명시 기존 경로 연결. 미완료 기능 메뉴는 숨기거나 이유를 표시하며 실행 가능한 것처럼 만들지 않는다.

- 접점: `src/cli.rs`, `src/lib.rs`, `src/setup/{paths,store,workflow_store}.rs`, `src/engine/{instance,instance_fs}.rs`; 새 `operator/{catalog,home}.rs`.
- 홈 조회는 무쓰기다. 새 설치/명시 목록 저장에서만 관리 루트와 필요한 부모를 검증·준비한다. 기존 루트는 검사하며 자동 chmod·채택하지 않는다.
- 카탈로그는 opaque ID와 검증된 경로/identity를 사용한다. 중복 표시 이름은 구분하며 전역 instanceId 유일성을 가정하지 않는다.
- 누락·손상·동시 갱신·카탈로그 기록 실패가 실제 노드 상태를 바꾸지 않는다. 목록에서 빼기와 제거/중지는 다른 동작이다.
- 기존 지원 세션/등록 연결은 읽기·목록 작업이다. 재등록·DB 채택·V1/V2 migration·임의 CLI exec는 없다.
- 목록은 마지막 관측임을 표시하고, 선택한 노드만 명시적으로 새로 확인한다. 지원 형식/CLI 일치에 따른 기능 가용성을 표시한다.

**검증/완료 기준:** 신규 `tests/console_catalog.rs`와 저장소 단위 검사. 첫 홈 무쓰기, public/secret 경로 누출 방지, 공백/Unicode 경로·symlink·다른 UID·inode 변경·중복 이름·색인 손상·갱신 충돌·상한, 실제 등록 완료 후 색인 쓰기 실패를 다룬다. 기존 CLI로 등록을 열 수 있는 fallback도 확인한다. 기본 위치의 부모 생성은 설치/DB 생성과 구분해 검사한다.

## UX-03. 자료 확인부터 제품·network 설정 연결

**산출물:** S02~S04의 준비·설정·검토. 아직 승인하지 않은 설치/DB/service는 생성하지 않는다.

- 접점: `src/setup/{mod,network,paths,workflow}.rs`, `src/config/{mod,network,preflight}.rs`, `src/engine/{product,setup_plan}.rs`.
- 기존 package/lock/독립 신뢰키와 chain/network/credential 참조를 역할별로 확인한다. 새 bundle 계약은 만들지 않는다.
- 승인 목록에서 자기 노드를 고르고 제품 입력을 한 번 준비한다. `setup --network` 내부 준비 API로 native를 만들거나 명시된 기존 native를 대조한다.
- 모듈 API가 부족하면 입출력/검증 경계를 추출한다. 기존 CLI 문답을 가짜 키 입력이나 셸 subprocess로 연결하지 않는다.
- 기본 새 경로를 제안하고 고급 경로를 검증한다. workflow/package/control/data overlap 및 network output 분리 조건을 유지한다.
- 공개 설정·대상·정책·자료 상태를 검토하고 항목 하나를 수정한다. 영향을 받는 하위 입력과 계획은 무효화하며 기존 immutable 파일은 새 revision으로 대체 준비한다.
- 새 설정 파일과 설치 session의 운용상 보존 조건을 명시한다. 기존 키·native·chain을 임의로 관리 루트에 옮기지 않는다.

**검증/완료 기준:** 제품/native 일치, 공개 목록의 자기 노드 선택, 잘못된 node/validator ID·peer/pin·주소, 누락 자료별 설명, 권한, 상대경로 기준, 기본 위치 충돌, 계획 후 자료 변경을 검사한다. 항목 수정은 다른 답을 보존하고 필요한 결과만 재생성하며 init/start는 0회여야 한다. 공급 자료 확인만으로 key 신뢰·runtime READY를 표시하지 않는다.

## UX-04. 통합 설치·재개 controller

**산출물:** S04 → S05 → S06, 등록 후 S08 연결. 설치·초기화·시작을 각각 승인한다.

- 접점: `src/setup/{workflow,workflow_store,install_wizard}.rs`, `src/install.rs`, `src/engine/instance.rs`, `src/engine/instance/supply.rs`; 새 `operator/install_flow.rs`.
- 새 console 세션이 기존 세션을 참조하는 조정 기록을 갖는다. 기존 API의 durable intent, ownership callback, confirmed/unknown 검사를 그대로 사용한다.
- apply 승인으로 init/start를 예약하지 않는다. 설치만 하고 종료하거나 초기화 뒤 종료하는 정상 경로를 제공한다.
- 재개는 입력·소유 결과·기존 실행을 읽고 가능한 다음 선택을 표시한다. 버튼을 누르지 않았는데 init/start를 재실행하지 않는다.
- 출력 이후 설정/등록 binding 편집은 차단한다. 완료된 session을 임시로 지우지 않는다. 목록 기록 오류는 실제 작업 완료와 분리한다.
- 설치 최초 start 이후의 반복 운용은 UX-05로 넘기며 설치 journal.start를 리셋하지 않는다.

**검증/완료 기준:** 확인 거절·EOF·화면 종료, intent/생성/결과 게시 전후 중단, 목록 저장 실패, 동시 세션, 입력 변경, 기존 session 재개를 검사한다. 관측만 하는 재개는 추가 installer/init/start 호출 0회다. 소유 증거 없는 partial을 채택하지 않고 UNKNOWN을 완료로 바꾸지 않는다. 코드 검사는 fixture와 fault injection으로, 실제 서비스/OS 중단 인수는 별도 근거로 기록한다.

## UX-05. 반복 운용 화면

**산출물:** S07~S09의 상태·같은 시도 대기·명시 중지·정상 종료 후 명시 시작.

- 접점: `src/engine/instance/service.rs`, `src/engine/launchd.rs`, `src/engine/runtime_http.rs`, `src/cli.rs`; 새 `operator/node.rs`.
- 기존 service API에 연결하며 설치 workflow 상태와 별도의 controller를 사용한다. 읽기 상태와 실행 의도 journal의 권한을 혼동하지 않는다.
- 상태에 대상 식별, 관측 시각, 로컬 준비 상태, 합의 미확인, LaunchAgent 세션 범위를 표시한다. 상세는 기존 reason/attempt/stage를 보존한다.
- 선택 후에도 실행 직전 상태·identity·lock·CLI/package 일치를 다시 검사한다. 목록의 cached READY로 실행하지 않는다.
- 확인창에서 선택한 root/binding identity와 expected attempt를 실행 guard에 전달한다. 현 `service::stop(instance, timeout)`의 latest 조회만으로는 충분하지 않으므로 필요한 내부 API를 확장하되 기존 개별 CLI 계약은 유지한다. 의도 게시·신호 대상 결속을 보장하고 확인창 뒤 대상 변경은 승인 폐기·재조회로 처리한다. 사용자 입력을 기다리며 service 잠금을 길게 잡는 방식은 사용하지 않는다.
- 정상 stop 확인 뒤 별도 start 확인을 받는다. stop timeout/GUI 부재/다른 시도/보고서 유실에서는 다음 start를 막는다.
- CLI mismatch는 검증한 기존 설치 CLI 안내로 처리한다. 새 console을 기존 archive에 덮어넣지 않는다. 지원 형식의 읽기/진단도 기존 검증 정책 안에서만 허용한다.

**검증/완료 기준:** 중복 start, operation busy, 같은 이름의 다른 control, CLI mismatch, 정상 stop→새 attempt/같은 DB, stop 지연, 상태 조회 예산·attempt 변경·stale 응답, 동시 콘솔/기존 CLI를 검증한다. 특히 확인창과 dispatch 사이 다른 CLI가 stop→start한 경우 새 attempt에 mutation 0회이며 이전 승인이 폐기되어야 한다. 화면 취소가 노드 종료를 뜻하지 않음을 확인한다. 실제 Mac logout/sleep 인수는 가능한 격리 환경에서 별도 계획·기록하며 UI fixture만으로 PASS 표시하지 않는다.

## UX-06. 오류에서 수정·지원까지

**산출물:** S10/S11의 reason별 안내, 실패한 항목 수정, 최근 기록, 새 정제 보고서 저장.

- 접점: `src/cli.rs`, `src/error.rs`, `src/engine/instance/diagnostics.rs`, `src/engine/{diagnostic_io,diagnostic_events}.rs`; 새 `operator/support.rs`.
- 허용된 reason code를 “입력 수정 / 관측 재시도 / 종료 확인 / 초기화 재개 조건 검사 / 지원 검토”로 매핑한다. 모르는 코드는 안전한 읽기·지원 대상으로 남긴다.
- 아직 미실행인 입력 오류와 결과 불명을 분리한다. 지원 검토가 필요한 상태에서 실행을 반복하도록 권하지 않는다.
- `resume-init`은 기존 조건 검사에 통과할 때만 별도 승인으로 실행하며 UNKNOWN 일반 복구 버튼을 만들지 않는다.
- 보고서 이름/부모를 안전하게 제안하고 existing file·control/data/reference overlap을 거부한다. 자동 전송하지 않는다.
- 최근 저장 기록과 live 관측을 구분하고 partial/파일 생성 여부를 독립 표시한다. raw/follow·전체 시도 보존/정리는 이번에 추가하지 않는다.

**검증/완료 기준:** secret canary, 과대/손상 자료·예산 초과, 출력 충돌, 기록은 있으나 live 불명, 보고서 생성 실패/partial, startup 진입 단계의 해석을 검사한다. 기본 화면에 private credential·원문 engine 출력이 새로 노출되지 않아야 한다. 지원 경로에서 엔진 재실행·네트워크 전송·자동복구 0회다.

## UX-07. 새 패키지 인수와 사용자 과업

**산출물:** 새 console CLI를 포함한 고정 signed-development archive, 새 실제 인수 결과, 현재 사용자 가이드, 작성자가 아닌 사용자 과업 기록.

1. 각 코드 PR은 `make check`를 통과한다. 필요한 회귀는 기존 `tests/` 및 `tests/acceptance/` 구조에 추가한다. 개발 중 HTML 시안 검사만으로 Rust/TTY 검증을 대체하지 않는다.
2. NIGO RC.3/JRE를 유지할 경우에도 **BXDL CLI 변경으로 package hash는 바뀐다.** 새 CLI/archive/source/toolchain/harness/inputs를 고정하고 새 control/data로 인수한다. 과거 accepted archive에 바이너리를 덮어쓰지 않는다.
3. 실제 PTY로 새 설치, 항목 수정, 취소/EOF, 대기 중 HUP, 재개, 기존 공급 경로 부재, 정상 stop/start와 no-extra-mutation을 검증한다. 제품 UX의 초기화·start·stop 횟수, 동일 DB/genesis, 잔존 PID/job/LISTEN을 독립 대조한다.
4. 공통 설치·서비스 경로가 바뀌므로 같은 새 archive의 4-validator 회귀 범위를 영향도로 결정한다. G1-M 후보로 제시하려면 해당 network 근거도 새 artifact와 연결해야 한다. 이전 RC.3 package PASS를 새 archive 전체 PASS로 복사하지 않는다.
5. 현재 개발 환경 도움 없이 package+문서만 사용하는 인수와 처음 사용자 과업을 분리 기록한다. 실제 OS 세션 중단/네트워크 차단은 격리된 인수 환경에서만 수행한다.

### 처음 사용자 과업

| ID | 과업 | 제안 성공 기준 |
| --- | --- | --- |
| T1 | 승인 자료로 새 설치 | 필요한 자료와 요청 담당을 찾고 개발자 구두 도움 없이 로컬 READY까지 진행. 초기화/시작 차이를 설명 |
| T2 | 잘못된 파일 하나 수정 | 실패 항목을 찾고 다른 답 재입력 없이 수정·재검사. 필요한 계획만 새로 검토 |
| T3 | 중단 후 다음 날 재개 | 목록에서 작업을 찾아 저장 단계 확인. 승인하지 않은 추가 init/start 없음 |
| T4 | 새 터미널에서 내 노드 운용 | 대상 구분 후 정상 stop·같은 DB start. 터미널 종료와 로그아웃의 차이를 설명 |
| T5 | 지연/실패 후 다음 행동 | 대기 만료를 종료로 오인하지 않고, 자료 오류·지원 검토 필요를 구분해 조회/정제 보고서 생성 |

최소 두 명의 작성자가 아닌 검토자에게 같은 과업을 맡기는 것을 1차 목표로 둔다. 자동화 에이전트의 PTY 수행은 이 사람 과업을 대신하지 않는다. 외부 검토자가 없으면 해당 항목을 NOT_RUN으로 두고 다른 검증을 계속한다. 완료 여부·도움 요청·오선택·되돌림·소요 시간을 기록하며 아직 근거 없는 시간 SLO는 두지 않는다. 데이터 삭제·재초기화·타 노드 조작 같은 중대한 오선택은 발생하지 않아야 하며, 설명이 필요했던 지점은 설계로 되돌려 재검증한다.

**완료 판단:** 기술 회귀·새 artifact 실인수·실제 수행한 사용자 과업을 각각 판정한다. 사람이 수행하지 않은 과업이나 OS 인수를 완료로 합산하지 않는다. 이 단계 완료도 전체 G1-M·고객 출시 승인과 같지 않다.

## 3. NIGO 의존성과 이후 작업

| 항목 | 판단 |
| --- | --- |
| 통합 설치/운용 UI | 현재 RC.3 계약으로 착수 가능. 새 NIGO 구현/회신 대기 없음 |
| engine/JRE 후보 변경 | 별도 명시 선택과 package 재인수 필요. UX 편의를 이유로 자동 갱신하지 않음 |
| CLI 호환/카탈로그/관리 경로 | BXDL 책임. NIGO가 UI나 새 registry를 구현할 필요 없음 |
| UNKNOWN 수동 조정 | 이번 범위 밖. 실제 엔진 증거/계약 변경이 필요하면 재현 후 별도 요구·설계 |
| J5·고객 배포 조건 | UX와 별도 공급망/허가 조건. 출처·SBOM·NOTICE·대응 소스·최소 OS 확정 필요 |
| A7 Storage | 공식 도구는 수신했으나 BXDL wrapper/인수는 별도 G2 작업. 이번 UX 선행 아님 |

기존 우선순위 앞에 UX-01~07을 둔다. 실패·중단과 Mac lifecycle 인수는 해당 UX 단계와 연결하고, **안전 제거 BX-034/REM-01은 별도 구현이어도 G1-M 전 필수**로 유지한다. A7·업데이트는 G2, Linux/systemd·Docker·장기 운영 LaunchDaemon은 후속이다. 이 설계만으로 REQ-0002/0003 OPEN·development·PROPOSED를 승격하지 않는다.

## 4. 각 PR에 남길 근거

적용한 UX ID/화면 ID, 변경한 사용자 행동, 기존 계약에 미치는 영향, 실제 실행한 검사와 결과, 미실행 과업, package/CLI identity 영향, 다음 단계 의존성을 한국어 PR에 기록한다. 소스·문서·시안 변경과 엔진 실제 실행 근거를 구분한다. 원문 키·설정·PTY·고객 자료는 커밋하지 않는다.
