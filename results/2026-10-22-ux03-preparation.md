# UX-03 — 직접 CLI 간소화와 설치 자료 준비 검증

- 일자: 2026-10-22
- 판정: **구현·로컬 회귀·Mac PTY PASS**. 새 실제 package/engine/service 인수는 NOT_RUN.
- source: 로컬 `cbf5c37` 위 UX-02 작업에 UX-03을 추가한 미커밋 작업 상태. PR/원격 CI 결과가 아니다.
- 연결: [설계](../design/2026-10-22-cli-simplification.md), [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md), [사용 가이드](../docs/preparation.md).

## 구현 범위

`bxdl setup new <이름> [--home <root>] [--from <instance.json>]`와 `bxdl setup resume <이름> [--home <root>]`가 console 홈 1번과 동일한 `operator::install_flow`를 사용한다. 새 명령은 이름 있는 자료 준비 세션, 제품 초안, 공급 자료, 공개 network 또는 기존 native의 준비·검토·재개까지 제공한다. 설치·등록·초기화·서비스 시작은 호출하지 않는다.

`setup::preparation::Preparation`은 기존 WorkflowStore의 private container·identity·exclusive lock·append-only 저장을 재사용하되 journal kind는 별도의 `BXDL_INSTALL_PREPARATION`이다. 기존 Draft/Network/Workflow/instance 저장 schema는 변경하지 않는다. 관리 root에는 승인 시 `sessions`만 추가하며 package/control/data 예정 폴더는 생성하지 않는다. 세션은 열려 있는 동안 해당 세션의 배타 잠금을 유지하고, 카탈로그 writer lock은 계속 짧게 사용한다.

공개 노드 목록 검증을 기존 network 렌더와 공유한다. 자기 노드의 nodeId·P2P 주소/포트는 함께 저장하고 정책은 답마다 저장한다. 다섯 정책 모두를 명시 선택해야 full options가 구성된다. 제품·자료 변경은 다른 답을 보존하고 검토 결과를 무효화한다. 같은 답 재입력은 무효화하지 않는다.

승인한 준비는 실제 archive 검증과 명시 서명 정책, engine lock/package, 제품/native/참조 대응을 검증하고 `generated/instance-NNNN.json`, `node-NNNN.json`을 새 파일로 게시한다. 원본 참조·파일 identity·hash를 재검사한다. 실패한 revision/출력은 보존하고 새 승인에서 새 revision을 사용한다. `PREPARED_RECORDED`는 저장 기록이며 현재 재검증과 실행 준비를 구분한다. package CLI mismatch는 준비 안내에 남기고 실제 설치 허용으로 전환하지 않는다.

`PreparationSession`을 기존 탐색 카탈로그에 명시 저장·조회할 수 있다. 준비 세션은 자동으로 카탈로그에 추가하지 않으며 관리 `sessions`의 제한된 이름 목록과 catalog 목록은 구분한다.

## 실행한 검사

| 검사 | 결과 | 범위 |
| --- | --- | --- |
| `make check` | PASS | Rust **464** (unit375, CLI16, CLI integration2, catalog17, preparation CLI20, setup7, install14, network8, safety5), Python50, fmt, Clippy `-D warnings` |
| `cargo check --locked --target x86_64-unknown-linux-gnu` | PASS | Linux 대상 타입 검사. Linux 런타임/systemd 인수 아님 |
| 실제 Mac PTY | PASS | 아래 명명된 생성/재개/취소·수정 경로 |
| 문서 링크·fence·`git diff --check` | PASS | 변경 문서·새 문서의 로컬 참조와 형식 |

UX-02의 427 Rust 대비 37개 추가다. Preparation 단위14개, 대상 투영2개, catalog1개, 신규 CLI20개이며 기존 network 검증15개도 전체 unit에 포함해 회귀했다. 숫자는 이 작업의 단일 `make check` 합계이며 필터 실행 결과를 중복 가산하지 않았다.

### 주요 회귀 근거

- 잘못된 이름·혼용 옵션·비TTY·JSON은 입력을 소비하거나 폴더를 만들지 않는다. help-only 역시 무입력·무쓰기다.
- 승인 거절·빈 Enter·EOF·취소 시 신규 홈 부재 또는 기존 저장 자료 보존을 확인했다. 중복·unmarked home·누락 재개는 채택/덮어쓰지 않는다.
- 제품 가져오기 참조 기준, 홈 쓰기와 data/reference 충돌, 기본 경로 미생성, private 검토 출력, source bytes 보존을 확인했다.
- 미완성 초안도 개별 수정할 수 있고, :back은 검토로 돌아온다. 부분 정책 답은 중단 후 유지되며 답하지 않은 정책을 승인으로 채우지 않는다.
- network 목록의 자기 노드 선택·native 전환·정책 재개, input/native 불일치, 자료 누락·변경·동일 bytes 파일 교체, 저장 충돌·review 변조·실패 revision을 검사했다.
- 실제 검증 가능한 작은 시험 archive와 lock을 사용한 injected CLI 준비→재개→HTTP 포트 수정→revision2 생성이 통과했다. revision1과 원본은 보존되고 예정 package/control/data는 생성되지 않았다. 이 시험 archive는 실제 NIGO/JRE 제품 인수 artifact가 아니다.
- Preparation 저장 기록의 catalog 조회는 현재 pin/서비스 관측을 수행하지 않는다. held-lock 거절·lock 해제·자료 부재 상태의 공개 저장 요약·원본 inode/bytes/mode 보존·대상 교체 거절을 확인했다.

### 실제 Mac 터미널

현재 debug CLI를 stdin/stdout/stderr PTY에 연결하고, 격리한 임시 관리 위치에서 다음을 수행했다.

1. 새 준비 생성의 도움말·Enter 거절 후 home 부재.
2. 승인한 private 이름 세션 생성 후 정상 종료.
3. 직접 `setup resume`의 무쓰기 재개.
4. console 홈 1번에서 같은 세션 재개 후 bytes/inode/mode 보존.
5. 미완성 초안의 HTTP 포트 개별 수정 후 새 프로세스에서 보존 확인.
6. 실제 terminal EOF로 기존 세션 중단 후 기록 보존.
7. 생성 확인 전 EOF로 새 세션 부재.
8. package/control/data 및 generated 출력 부재, 새 디렉터리0700·파일0600 확인.

이 PTY 실행은 엔진·launchd·HTTP를 호출하지 않았다. 임시 시험 디렉터리는 정리했고 실제 사용자 노드나 기존 accepted archive는 변경하지 않았다. 전체 원문 터미널 기록이나 credential은 저장소에 추가하지 않았다.

## 후속과 미검증

- UX-04: 승인한 준비 결과를 기존 install workflow에 연결하고 설치/등록·init·start를 각각 승인. 관측 재개에서 추가 mutation 0회, 실제 작업 완료 뒤 catalog 실패 분리.
- UX-05: `nodes`, 고유 명령 별칭의 명시 지정, `status/start/stop/logs <alias>`, console 반복 운용. 현재 표시명을 명령 alias로 사용하지 않는다.
- UX-06/07: 별칭 diagnose, 과업별 도움말·셸 완성, 새 CLI package 실인수와 사람이 수행하는 과업.
- 새 RC.3/JRE archive 실인수, 실제 mutation crash·Mac logout/sleep·다른 OS 사용자 실행, 작성자가 아닌 사람의 사용성 검증은 **NOT_RUN**. 이전 RC.3 PASS를 새 CLI에 재사용하지 않았다.
- NIGO 저장소·engine lock·JRE 선정과 공급 파일은 변경하지 않았다. 새 NIGO 선행 구현은 필요하지 않았다.
