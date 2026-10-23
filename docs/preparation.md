# 이름으로 시작하는 순차 설치 준비와 실행

직접 `setup new/resume`은 필요한 자료와 누락된 설정을 순서대로 묻는다. `setup list/show`로 저장 상태를 조회하고 `setup edit`으로 제품 항목 하나를 수정할 수 있다. console의 추가 개발은 중단했으며 기존 번호 메뉴는 보존한다.

`setup new/resume`은 **설치 자료와 제품/native 설정을 준비**한다. 준비를 마친 뒤 `setup apply <이름>`을 별도로 실행하고 최종 승인하면 파일 설치·cold 검사·등록·노드 이름 연결을 수행한다. 초기화와 서비스 시작은 각각 `init`, `start`로 실행한다. 준비 조회·수정·재개만으로 설치하거나 노드를 시작하지 않는다. 기존 `setup --install` 흐름도 유지한다.

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

개발 checkout에서 실제 설치를 시험할 때는 `make build` 뒤 `bin/bxdl`을 실행한다. release 빌드는 대용량 package의 반복 무결성 검사 비용을 줄인다. 기존 RC.3 archive의 CLI에는 새 명령이 없으며 바이너리만 덮어쓰지 않는다. [독립 관리 CLI](./management-cli.md)를 패키지 밖에서 업데이트하고 기존 RC.3 실행기를 사용한다. Mac 기본 관리 위치는 `$HOME/Library/Application Support/BXDL`이다. 기존 `setup`만 들어 있는 unmarked 폴더는 자동 채택하지 않으므로 새 `--home`을 지정한다. 다른 OS는 `--home`을 명시하며 Linux 서비스 제공을 뜻하지 않는다.

이름은 `^[a-z][a-z0-9-]{0,31}$`다. 같은 관리 위치에서 고유한 준비 세션 이름이며 엔진 instanceId와 운용 별칭은 별개다. 기존 이름은 `resume`으로 열고 새 세션으로 덮어쓰지 않는다. 기존 제품/network/install 세션은 해당 기존 `--resume` 명령을 사용한다.

새 세션 생성은 경로를 보여준 뒤 `y`/`yes`로 확인한다. Enter·거절·취소·EOF는 만들지 않는다. 승인하면 관리 root와 `sessions/<이름>`의 private 준비 기록만 만들며 `packages`, `instances`, `data`의 운영 폴더는 생성하지 않는다. 최초 제안 위치는 각각 `<home>/packages/<이름>`, `<home>/instances/<이름>`, `<home>/data/<이름>`이다. 가져온 제품의 데이터 위치는 유지한다. 관리 홈 안에 출력할 경우 package/control/data는 각각 `packages`, `instances`, `data` 바로 아래에 둔다. `catalog`·`sessions`·`operations` 등 관리 기록 공간과 잘못된 저장 공간은 적용 전에 거절한다. 외부 출력 위치는 부모 폴더를 미리 준비한다.

`new/resume/edit/apply`는 TTY 전용이며 `--json`, `--non-interactive`는 입력·쓰기 전에 거절한다. `list/show`는 비TTY와 `--json`을 지원한다. 각 명령의 `--help`로 사용법을 확인할 수 있다. 기존 제품 초안 명령의 JSON·종료 코드는 유지한다.

## 필요한 항목부터 묻는 순서

1. 공급 package archive, 외부 신뢰 공개키 또는 명시적인 개발 정책, engine lock을 지정한다.
2. 공통 chain/network에서 내 노드를 선택하거나 제공된 native 설정을 지정한다.
3. 가져온 제품 설정과 저장한 답을 재사용하고, 빠진 제품 필드·credential 참조·준비 정책을 입력한다.
4. 공개 설정, 선택 자료와 설치·제어·데이터 예정 위치를 검토한다. 필요하면 해당 항목만 수정한다.
5. 설정 파일 준비를 별도로 승인한다. 검증한 제품/native revision을 저장하고 결과와 다음 명령을 안내한 뒤 종료한다.

검토 화면에서는 다음 행동을 직접 입력한다.

```text
y / yes           검증하고 설정 파일 준비
Enter / n / no    현재 답을 보존하고 종료
edit httpPort     제품 항목 하나 수정 후 검토로 복귀
edit archive      공급 패키지 경로 수정 후 검토로 복귀
details           공급·공개 입력과 예정 출력의 상세 경로 확인
```

검토의 `edit`은 제품 필드 외에 `archive`, `trust`, `lock`, `source`, `network`, `native`, `node`, `destination`, `instance`와 공개 목록의 정책 필드 `keyStoreType`, `trustStoreType`, `mode`, `consensusAutoStart`, `archiveEnabled`를 받는다. 기존 native를 사용하는 경우 정책은 그 파일에 따르며 직접 수정하지 않는다. 이는 아래의 제품 전용 `setup edit --field`와 구분한다.

이미 준비한 세션을 재개하면 저장 자료를 다시 대조하고 검토 화면으로 간다. 유효한 결과가 그대로라면 `y`로 확인해도 새 revision을 중복 생성하지 않는다. 자료가 바뀌거나 기존 검증에 실패한 경우 다시 준비하려면 명시적인 승인이 필요하다.

재개하면 현재 완료 여부와 다음 필요한 입력을 확인하고 누락된 질문부터 이어간다. 이미 저장한 답을 다시 모두 입력할 필요가 없다. `:help`는 현재 질문의 설명, `:back`은 앞 질문으로 이동, `:cancel`/EOF는 저장된 답을 보존하고 중단한다. 검토 중 수정은 한 항목을 바꾼 뒤 검토로 돌아온다.

네트워크 목록은 체인 membership, node/validator ID, endpoint 중복, 거래 권한, active/revoked pin 형식을 검증한다. 자기 노드 선택은 nodeId·P2P 주소/포트를 함께 저장한다. 로컬 P2P listen 주소는 제품 schema의 unicast IP 제약을 따른다. 공개 목록을 사용할 때 TLS 형식 두 개·시작 정책·합의 참여·아카이브 정책은 답마다 저장하며, 다섯 항목을 모두 선택해야 새 native를 준비한다. Enter는 표시한 정책 기본값을 선택하지만 파일 생성 확인의 Enter는 거절이다.

제품 입력과 자료·정책 변경은 다른 답을 보존하면서 이전 준비 검토를 무효화한다. 변경 없는 답은 기존 결과를 유지한다. 기존 키·chain·native는 옮기거나 수정하지 않는다. 비밀번호는 원문이 아닌 기존 파일 경로만 입력한다. 검토 출력은 credential 경로를 표시하지 않는다.

## 준비한 자료로 설치 실행

```bash
bxdl setup show local
bxdl setup apply local
# 별도 관리 위치를 사용했다면 같은 --home을 지정한다.
bxdl setup apply local --home '/absolute/BXDL-lab'
```

`apply`는 준비된 제품/native revision과 공급 자료, package·control·data 위치, 등록 후 연결할 이름을 검토한다. 최초 실행에는 준비 자료 전체가 유효하고 설치 대상이 새 위치여야 한다. 화면의 최종 확인에서 `y` 또는 `yes`로 승인한다. Enter·`n`·`no`는 실행을 거절하며 `:cancel`/EOF는 저장 자료를 보존하고 중단한다. `--json`과 비TTY는 입력·쓰기 전에 거절한다. 자동화용 일괄 승인 옵션은 제공하지 않는다.

승인한 작업 범위는 **패키지 파일 설치 → NIGO cold 검사 → control 등록 → 같은 이름으로 노드 연결**이다. DB 초기화·LaunchAgent 등록·노드 시작은 포함하지 않는다. 등록이 끝났어도 cold의 알려진 INCOMPLETE를 READY로 바꾸지 않는다. 다음 단계는 별도 명령이다.

```bash
bxdl init local --confirm-initialize
bxdl start local
bxdl status local --wait
```

준비 세션은 `<home>/sessions/<이름>`, 설치 실행 기록은 `<home>/operations/<이름>`에 따로 저장한다. 준비 폴더를 설치 workflow로 변환하거나 이전 JSON 형식을 덮어쓰지 않는다. 실행 기록은 승인한 준비 revision·파일 identity·hash·대상 위치에 결속되고 기존 workflow의 잠금·durable intent·소유 근거를 재사용한다. 준비 세션의 `generated/` 제품/native 파일은 등록 후에도 운용 입력이므로 보존한다. 실행 기록도 임시 폴더처럼 삭제·이동하지 않는다.

동일한 `apply`를 다시 호출하면 기록과 현재 대상을 먼저 확인한다. 이미 설치·등록을 완료한 실행은 완료 근거를 검증하고 필요한 이름 연결만 복구한다. 패키지를 재설치하거나 엔진을 다시 등록·초기화하지 않는다. 이름 저장 실패는 설치 실패와 구분한다. 기존 control 경로로 `instance show --instance <경로>`를 조회할 수 있으며 이름 충돌을 다른 노드로 덮어쓰지 않는다.

준비 자료가 바뀐 `SOURCE_CHANGED`, 설치 의도 뒤의 `UNKNOWN`, 소유 근거를 확인하지 못한 부분 설치는 자동 재설치·재등록하지 않는다. 기존 실행 기록과 파일을 보존하고 `setup show`의 실행 사유를 확인한다. `setup resume`으로 답을 바꾸는 것은 이전 실행을 새 입력에 다시 연결하는 승인이 아니다. 기존 폴더·DB의 임의 채택, 자동 cleanup과 복구를 제공하지 않는다.

새 V2 등록이 끝난 뒤 archive·신뢰 공개키·engine lock은 control의 검증된 보관본으로 확인할 수 있다. 완료 실행을 재확인할 때 원래 공급 파일이 없더라도 기록과 보관본의 검증을 수행한다. 반면 제품/native·chain·credential과 준비 revision·설치본·control·data는 계속 보존해야 한다. 원본 부재와 준비 기록 자체의 변경은 구분한다.

## 준비 목록과 설치 실행 기록 조회

```bash
bxdl setup list
bxdl setup show local
bxdl setup list --home '/absolute/BXDL-lab' --json
bxdl setup show local --home '/absolute/BXDL-lab' --json
```

`list`는 지정한 관리 위치의 준비 세션을 나열하고 별도의 설치 실행 기록도 함께 표시한다. 관리 위치가 아직 없으면 빈 목록을 반환하며 폴더를 만들지 않는다. 관리 위치의 `sessions`와 `operations`에 남은 유효한 이름을 합쳐 준비 단계·부족한 입력·설치 실행 기록을 보여주며 외부 공급 자료를 다시 검증하지 않는다. 기록 손상·일반 파일·symlink 등으로 해당 이름을 정상 세션으로 읽을 수 없으면 `UNAVAILABLE`로 표시하며 나머지 목록은 유지한다. 잘못된 이름은 준비 세션 목록에 포함하지 않는다.

`show`는 선택한 준비 세션의 공개 설정, 부족한 입력과 다음 행동을 표시한다. 설치 실행 기록이 없으면 공통 network의 자기 노드 선택과 저장된 준비 결과를 현재 참조 자료와 대조한다. 실행 기록이 있으면 준비 요약과 별도로 설치 실행의 source 연결·소유한 설치 파일·등록 완료 근거를 검증한다. 이때 원래 공급 archive/key/lock의 재검사를 준비 검증으로 중복 요구하지 않는다. 두 기록을 조회하면서 설치·등록을 재개하지 않는다. **저장 기록 `PREPARED_RECORDED`, 현재 준비 자료 검사, 설치 실행 기록은 별개다.** 설치 완료 기록은 현재 서비스 상태가 아니며 노드 운용은 `status <이름>`으로 확인한다. 과거 성공 기록만으로 지금 사용할 수 있다고 판단하지 않는다.

| 현재 검사 결과 | 의미 |
| --- | --- |
| `NOT_PREPARED` | 아직 승인해 저장한 준비 결과가 없음 |
| `NOT_CHECKED` | 저장 내용만 조회했으며 외부 자료는 재검증하지 않음 |
| `VERIFIED` | 조회 시점의 자료와 저장 준비 기록이 일치함. 엔진 실행 검증은 아님 |
| `CHANGED` | 저장 결과를 현재 자료와 대조하지 못함. 누락·변경·충돌 등을 확인해야 함 |

두 조회 명령은 세션·설정·출력 파일을 쓰거나 잠금 파일을 생성하지 않으며, 작업자의 배타적 준비 잠금을 획득하지 않는다. 파일 읽기에 따른 접근 시각 변경은 가능하다. credential 경로, 원본 답변 목록, 키 내용은 JSON에도 포함하지 않는다. 동시 변경이나 대상 교체를 확인하면 유효한 완료 상태로 추정하지 않는다. 관리 위치 자체가 안전하지 않으면 조회 오류로 종료한다. 준비 세션이 삭제되거나 손상되어도 별도 설치 실행 기록을 읽을 수 있다면 이를 숨기지 않는다. 준비 읽기 오류와 실행 기록을 함께 표시해 기존 설치·등록 및 보존할 control을 확인할 수 있게 한다.

## 제품 항목 하나 수정

```bash
bxdl setup edit local --field httpPort
bxdl setup edit local --field chainDescription --home '/absolute/BXDL-lab'
```

`--field`에는 기존 제품 설정 필드명 하나를 지정한다. 해당 질문만 받고 저장 후 종료하며 다른 답은 유지한다. 이미 저장한 항목에서 Enter는 기존 값을 유지한다. `:back`은 변경 없이 정상 종료하고 `:cancel`/EOF는 변경 없이 중단한다. 유효하지 않은 값은 같은 질문에서 다시 받는다.

제품 필드 수정은 완성된 준비 결과에도 가능하다. 실제 값이 바뀌면 이전 검토를 무효화하지만 기존 `generated` 파일은 보존한다. 다음 `setup resume local`에서 변경 내용을 검토하고 새 설정 revision 준비를 승인한다. `edit` 자체는 설정 파일 준비를 자동 실행하지 않는다. 변경 없는 값은 준비 결과와 checkpoint를 그대로 유지한다.

`setup edit`은 제품 필드만 받는다. archive·신뢰 정책·engine lock·network/native·설치/제어 위치는 `setup resume`의 검토에서 수정한다. 비밀번호 원문을 명령 인자로 받지 않는다.

## 저장·검증과 결과 해석

설정 준비 승인은 archive 전체 검증·명시한 서명 정책·engine lock/package identity·제품/native 대응·입력 참조와 출력 충돌 검사를 실행한다. 공개 목록으로 생성하거나 제공된 native의 상대 참조를 원본 위치에 고정하여 `generated/instance-NNNN.json`, `generated/node-NNNN.json`에 저장한다. 검증은 참조 credential 파일을 읽고 private pin을 기록할 수 있으나 키·인증서의 실제 identity 또는 엔진 수용 여부는 검증하지 않는다.

실패 시 일부 새 revision 파일이 남을 수 있다. 이를 성공으로 채택하거나 덮어쓰지 않고 다음 명시 승인에서 새로운 revision을 사용한다. 설정 준비 실패를 자동 재시도하지 않는다. 생성된 설정과 세션은 계속 보존한다.

관리 CLI와 package 실행기의 바이트가 다를 수 있다. 지원하는 RC.3 profile은 실제 설치·운용 시 기존 실행기로 위임하며, CLI UX 변경만으로 노드 package를 다시 만들 필요는 없다. 준비 성공은 engine cold 검사·DB 생성·서비스 READY·네트워크 합의 성공이 아니다. NIGO RC.3/JRE pin은 유지한다.

| exit | 이름 있는 설치 준비 명령에서의 의미 |
| --- | --- |
| 0 | 요청한 조회·답변 저장·설정 준비·정상 종료 완료. 미완성 초안 조회도 성공이며 설치 완료를 뜻하지 않음 |
| 2 | 인자 오류 또는 문답 명령의 비TTY/JSON 사용 |
| 3 | 세션 읽기·저장·자료 검증 실패 |
| 5 | 취소/EOF 중단, 목록 일부 확인 실패, 저장 준비 결과의 현재 재검증 실패 |
| 7 | 입력·안내·결과 출력 실패 |

JSON 조회는 기존 공통 envelope 안에 `data.schemaVersion=1`인 준비 요약을 넣는다.

| 명령 | 주요 `data` 항목 |
| --- | --- |
| `list` | `home`, `partial`, `entries[]`. 각 entry에 `name`, `record=READABLE/UNAVAILABLE`, 선택 `inspection` 또는 `reasonCode`, 선택 `application`, `nextCommand` |
| `show` | `name`, 선택 `inspection`, 준비 읽기 실패 시 `reasonCode`, 선택 `application`, `nextCommand` |

`inspection`은 저장 요약 `summary`, 공개 필드 `fields`, 부족한 제품 필드 `missingFields`, 부족한 공급·정책 입력 `missingInputs`, 다음 입력 `nextInput`, 공개 노드 선택 상태 `nodeSelection`, 현재 검사 `verification`, 선택 `verificationCode`를 포함한다. `nodeSelection`은 `NOT_APPLICABLE`, `MISSING`, `NOT_CHECKED`, `MATCH`, `MISMATCH`, `UNVERIFIED` 중 하나이며 list는 현재 공개 자료를 읽지 않으므로 저장된 노드 값이 있어도 `NOT_CHECKED`다. 경로형 제품 값은 `선택됨`으로 표시하며 관리 위치와 자체 세션 경로만 노출한다. `nextCommand`는 셸 문자열 대신 인자 배열로 제공한다.

설치 실행 기록이 있으면 `show`의 `data.application`, `list`의 `data.entries[].application`에 별도 요약을 담는다. `record=READABLE|UNAVAILABLE`, 선택 `summary`, `nameBinding=CONNECTED|MISSING|CONFLICT|UNAVAILABLE`, `runtime=NOT_OBSERVED`, 선택 `reasonCode`를 제공한다. apply 결과도 같은 `application` 구조를 사용하며 이름 저장에 실패한 결과에는 `nameBinding=FAILED`와 사유가 포함된다.

실행 `summary`에는 `operationId`, `preparationRevision`, `instance`, `installation`, `registration`, `initialization`, `registrationConfirmed`, `sourceMatch=MATCH|MISMATCH|UNAVAILABLE`, `verification=NOT_CHECKED|PASSED|FAILED`, 선택 `verificationCode`가 있다. `initialization`은 실행 기록 당시 값이며 현재 노드 상태가 아니다. list는 실행 기록만 읽고 원자료를 검사하지 않는다. show는 소유한 설치 파일·완료 등록 근거를 엔진과 runtime 호출 없이 재검증한다. 완료 실행의 다음 초기화 안내는 현재 등록이 `NOT_STARTED`인 경우에만 제공하고, 이미 초기화 기록이 있으면 `status`를 안내한다. 완료 검증은 통과했으나 이름 연결이 누락된 경우 같은 `setup apply`로 검증·연결만 복구하도록 안내한다. 이름 충돌이나 읽기 실패는 부분 결과로 구분한다.

준비 자체의 `summary.installation=NOT_PERFORMED`, `engineValidation=NOT_CHECKED`, `runtimeReadiness=NOT_CHECKED`는 유지한다. 별도 설치 실행 요약을 준비 성공으로 덮어쓰지 않으며 조회 성공·설치 완료·현재 runtime 준비 상태를 구분한다.

## 기존 console과 운용 명령

console의 추가 개발은 중단했으며 **기존 번호 메뉴는 보존**한다. 직접 CLI와 console은 같은 Preparation 저장 형식을 사용하므로 서로 만든 준비 세션을 재개할 수 있다. console의 홈 1번은 기존 메뉴형 자료 준비를 연다. 목록 조회만으로 준비나 설치를 재시도하지 않는다.

준비만으로 노드 카탈로그에 추가하지 않는다. `setup apply`의 명시 승인 범위에 설치·등록 완료 후 같은 이름 연결을 포함한다. 이미 설치·등록된 노드는 `bxdl nodes add local --instance <control>`로 이름을 연결하고 `bxdl status/start/stop/logs local`로 운용한다. [노드 이름 가이드](./nodes.md), [CLI 간소화 설계](../design/2026-10-22-cli-simplification.md), [전체 구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)을 따른다.

CLI-03 준비 구현 검증은 [CLI-03 결과](../results/2026-10-23-cli03-sequential-preparation.md)에 기록했다. 자동 회귀와 실제 Mac 문답·조회 검증은 합성 준비 자료를 사용했으며 엔진·서비스 실행을 포함하지 않는다.

CLI-04 설치 연결·이름 기반 초기화의 자동 검사와 최종 RC.3 Mac 전체 흐름 12개 사례를 통과했다. [CLI-04 검증 기록](../results/2026-10-23-cli04-apply-and-lifecycle.md)에서 전체 검사·최종 경로 보호 회귀와 바이너리별 실제 인수 범위를 구분한다.
