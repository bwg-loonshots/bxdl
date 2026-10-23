# 이름으로 시작하는 설치 준비

**방향 변경:** console 개발을 중단하고 직접 CLI를 우선한다. 이 가이드는 현재 구현된 번호 메뉴 방식의 사용법이다. `setup new/resume`을 필요한 값만 순서대로 묻는 흐름으로 교체하고 `setup list/show/edit`을 추가하는 작업은 [CLI-03 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)이며 아직 구현하지 않았다.

현재 소스의 UX-03은 **설치 자료와 제품/native 설정을 준비하는 단계**다. 직접 명령과 console이 같은 Rust controller와 저장소를 사용한다. 실제 설치·등록·초기화·시작의 직접 CLI 연결은 CLI-04(종전 UX-04)이며, 기존 `setup --install`은 계속 사용할 수 있다.

## 시작과 재개

```bash
bxdl setup new local
bxdl setup resume local
```

별도의 관리 위치를 사용하거나 기존 제품 설정을 가져오려면 다음처럼 실행한다.

```bash
bxdl setup new local --home '/absolute/BXDL-lab' --from '/absolute/instance.json'
bxdl setup resume local --home '/absolute/BXDL-lab'
```

개발 소스에서는 `cargo build --locked` 뒤 `target/debug/bxdl`을 실행한다. 기존 RC.3 archive의 CLI에는 새 명령이 없으며 바이너리만 덮어쓰지 않는다. Mac 기본 관리 위치는 `$HOME/Library/Application Support/BXDL`이다. 기존 `setup`만 들어 있는 unmarked 폴더는 자동 채택하지 않으므로 이 경우 새 `--home`을 지정한다. 다른 OS는 `--home`을 명시하며 Linux 서비스 제공을 뜻하지 않는다.

이름은 `^[a-z][a-z0-9-]{0,31}$`다. 같은 관리 위치에서 고유한 준비 세션 이름이며 엔진 instanceId나 향후 운용 별칭과 별개다. 기존 이름은 `resume`으로 열고 새 세션으로 덮어쓰지 않는다. 기존 제품/network/install 세션은 해당 기존 `--resume` 명령을 사용한다.

새 세션 생성은 경로를 보여준 뒤 `y`/`yes`로 확인한다. Enter·거절·취소·EOF는 만들지 않는다. 승인하면 관리 root와 `sessions/<이름>`의 private 준비 기록만 만들며 `packages`, `instances`, `data`의 운영 폴더는 생성하지 않는다. 최초 제안 위치는 각각 `<home>/packages/<이름>`, `<home>/instances/<이름>`, `<home>/data/<이름>`이다. 가져온 제품의 데이터 위치는 유지한다.

`--help`는 `bxdl setup new --help`와 `bxdl setup resume --help`다. 새 문답 명령은 TTY 전용이며 `--json`, `--non-interactive`는 받지 않는다. 기존 명령의 JSON·종료 코드는 유지한다.

## 준비 순서

| 메뉴 | 행동 |
| --- | --- |
| 1 제품 설정 채우기·수정 | 빠진 제품 필드만 입력. 완성된 초안이면 한 항목 선택 수정 |
| 2 공급 자료 수정 | package archive, 외부 신뢰 공개키 또는 명시 unsigned-development, engine lock 선택 |
| 3 네트워크 준비 | 승인한 chain/network 목록에서 내 노드 선택 또는 기존 native 설정 선택 |
| 4 경로 수정 | 설치·제어·데이터의 새 위치 지정 |
| 5 검토하고 새 설정 파일 준비 | 별도 승인 뒤 자료 검증과 immutable 제품/native revision 저장 |
| 6 상세 경로와 재개 명령 | 준비 세션·공급/공개 입력·예정 출력 위치와 복사 가능한 재개 명령 |
| 7 제품 항목 하나 수정 | 미완성 초안에서도 이미 답한 chain·키 참조 등을 바로 수정 |
| 0 저장하고 종료 | 현재까지 저장한 초안을 보존하고 정상 종료 |

새 초안은 2번으로 공급 자료를 선택하고 3번으로 chain/network와 자기 노드를 고른 뒤, 1번에서 나머지 값을 채우면 공개 node ID·P2P 주소/포트를 반복 입력하지 않는다. 승인 주체를 자동 확인하는 기능은 없으며 운영자가 받은 공통 자료를 지정한다. 잘못된 chain은 7번에서 한 항목만 수정할 수 있다.

네트워크 목록은 체인 membership, node/validator ID, endpoint 중복, 거래 권한, active/revoked pin 형식을 검증한다. 자기 노드 선택은 nodeId·P2P 주소/포트를 함께 저장한다. 로컬 P2P listen 주소는 현재 제품 schema의 unicast IP 제약을 따른다. TLS 형식 두 개·시작 정책·합의 참여·아카이브 정책은 답마다 저장하고, 다섯 항목을 모두 명시 선택해야 새 native를 준비한다. Enter는 표시한 정책 기본값을 선택하지만 파일 생성 확인의 Enter는 거절이다.

기존 제품을 가져오면 값을 다시 입력하지 않는다. 제품 입력과 자료·정책 변경은 다른 답을 보존하면서 이전 준비 결과를 무효화한다. 변경 없는 답은 기존 결과를 유지한다. 기존 키·chain·native는 옮기거나 수정하지 않는다. 비밀번호는 원문이 아닌 기존 파일 경로만 입력한다. 기본 검토는 credential 경로를 표시하지 않는다.

## 저장·검증의 의미

기본 검토에는 신뢰 정책과 선택한 실행 옵션을 표시하며 5번의 승인 전과 6번 상세에서 공급/공개 입력·출력 경로를 검토한다. credential 경로는 상세에도 표시하지 않는다.

5번은 archive 전체 검증·명시한 서명 정책·engine lock/package identity·제품/native 대응·입력 참조와 출력 충돌 검사를 수행한다. 공개 목록으로 생성하거나 제공된 native의 상대 참조를 원본 위치에 고정하여 세션의 `generated/instance-NNNN.json`, `generated/node-NNNN.json`에 저장한다. 준비 검증은 참조 credential 파일을 읽고 private pin을 기록할 수 있으며 키·인증서의 실제 identity 또는 엔진 수용 여부는 검증하지 않는다.

`PREPARED_RECORDED`는 마지막 저장된 준비 결과다. 화면은 현재 자료와의 재대조 결과를 따로 표시한다. 누락·변경 시 이전 결과를 실행 근거로 삼지 않으며, 답변을 수정하고 다시 준비할 수 있다. 실패 시 일부 새 revision 파일이 남을 수 있다. 이를 성공으로 채택하거나 덮어쓰지 않고 다음 승인에서 새로운 revision을 사용한다. 생성된 설정과 세션은 계속 보존한다.

현재 도구와 package CLI가 다르면 준비 단계에서 알린다. 실제 설치·시작은 같은 CLI를 담은 새 package와 다시 검증해야 한다. 준비 성공은 engine cold 검사·DB 생성·서비스 READY·네트워크 합의 성공이 아니다. NIGO RC.3/JRE pin은 이번 변경에서 유지했다.

`:help`는 문맥 도움말, `:back`은 검토 화면 복귀, `:cancel`/EOF는 저장된 자료를 보존하고 중단한다. 정상 메뉴 종료 0, 취소/EOF 5, 인자·비TTY 오류 2, 저장·검증 오류 3, 입출력 실패 7이다. 검토 메뉴에서 고칠 수 있는 오류는 안내 후 메뉴에 남으며, 그 뒤 정상 저장 종료의 0은 준비 성공이나 설치 성공을 뜻하지 않는다.

## 기존 console 연결 — 추가 개발 중단

`bxdl console --home '/absolute/BXDL-lab'`의 **1번 → 새 준비 세션/준비 세션 재개**가 같은 흐름을 연다. 재개 화면은 해당 관리 위치의 `sessions` 아래 이름을 제한된 범위로 보여주고, 선택한 기록만 검증한다. 디스크 전체를 검색하지 않는다. direct CLI로 만든 세션을 console에서, console에서 만든 세션을 direct CLI로 열 수 있다.

홈 2번의 기존 카탈로그 목록은 명시 저장한 탐색용 목록이다. 준비 세션을 자동으로 카탈로그에 추가하지 않는다. 홈 4번의 **5 설치 준비 세션**으로 경로를 열어 명시 저장하면 기존 목록에 함께 표시할 수 있다. 목록 읽기는 자료 준비나 실행을 재시도하지 않는다.

`bxdl nodes`, `bxdl start local`, `bxdl status local`, `bxdl stop local`은 CLI-01/02 계획이다. 현재 운용은 기존 `--instance <control-path>` 명령을 사용한다. [CLI 간소화 설계](../design/2026-10-22-cli-simplification.md)와 [전체 구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)을 따른다.
