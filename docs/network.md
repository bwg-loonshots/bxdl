# 기존 키로 네트워크 설정 준비하기

`setup --network`는 제품 설정과 공통 공개 목록을 받아 NIGO RC.1~RC.3가 공유하는 native 계약의 `node.json`을 만든다. 승인된 chain과 기존 NGVK·TLS 자료를 사용한다. 키·인증서 발급이나 노드 실행은 수행하지 않는다.

## 먼저 준비할 자료

- 기존 [제품 설정](./setup.md)의 `instance.json`: canonical node ID(`0x` + 64자리 hex), 로컬 HTTP/P2P, chain·data·6개 secret 파일 참조.
- 운영자가 승인한 선택 후보의 chain JSON: QBFT validator membership를 포함한다. 이 도우미가 chain을 만들거나 보정하지 않는다.
- 전체 validator의 공개 목록: node ID, validator ID(`0x` + 40자리 hex), 연결 주소·port, 거래 권한, TLS leaf 인증서의 NIGO pin. 첫 버전은 validator만 1~128개 지원하며 chain membership와 정확히 같아야 한다.
- NIGO NGVK v1 signer, PKCS12/JKS key/trust store, 각각의 비밀번호 파일. 비밀번호 원문을 문답에 입력하지 않는다. TLS store와 private-key entry는 같은 비밀번호를 사용해야 하며 단일 PrivateKeyEntry 자료를 권장한다.

## 인증서 pin 계산

각 운영자가 실제 사용할 공개 **leaf 인증서 하나**를 PEM CERTIFICATE 또는 DER로 내보내 받아 계산한다.

```bash
bxdl config fingerprint --certificate ./node-a-leaf.pem --json
```

결과의 NIGO MTLS fingerprint를 공통 목록의 `credentialFingerprints`에 넣는다. 일반 인증서 SHA256과 다른 값이다.

```text
0x + hex(SHA256("NIGO_TRANSPORT_CREDENTIAL/v1" + NUL + "MTLS" + NUL + leaf DER))
```

이 명령은 공개 인증서 bytes에서 pin만 계산한다. X.509 의미·서명·신뢰·만료·keystore와의 일치를 검증하지 않는다. 개인키나 P12/JKS 파일을 인증서 대신 입력하지 않는다.

## 공통 공개 목록

`schemaVersion: 1`, `kind: "BXDL_QBFT_NETWORK"`와 `nodes` 배열을 가진 JSON이다. 각 노드의 필수 항목은 다음과 같다.

| 항목 | 의미 |
| --- | --- |
| `nodeId`, `validatorId` | 공개 node identity와 승인된 chain의 validator identity |
| `host`, `port` | 해당 peer의 연결 주소와 P2P port. 로컬 항목은 제품 P2P 주소·port와 일치 |
| `transactionPermission` | `NONE`, `RELAY`, `INGRESS` 중 명시 선택. local 항목은 로컬 거래 ingress에도 적용 |
| `credentialFingerprints` | 사용할 leaf 인증서의 NIGO pin 배열, 최소 1개 |
| `revokedCredentialFingerprints` | 선택한 폐기 pin 배열, active pin과 겹치면 안 됨 |

전체 node ID·validator ID·endpoint와 pin 소유자는 중복할 수 없다. 각 노드는 자기 항목을 제품 node ID로 선택하고 나머지를 peer로 렌더한다. 공통 목록에 키·비밀번호·로컬 파일 경로를 넣지 않는다. 저장소의 [network.development.json](../config/examples/network.development.json)과 [schema](../contracts/bxdl/network.schema.json)를 참고한다. 예시는 구조 예시일 뿐 실행 가능한 네트워크 자료가 아니다. 예시 pin과 identity를 실제 공개 자료로 바꾼다.

JSON 입력과 생성 native의 한도는 각각 256 KiB이며 active/revoked pin은 각 노드에서 각각 최대 16개다. 체인·secret 참조의 로컬 metadata 검사가 실패하면 생성 전에 중단하므로 실제 파일과 권한을 먼저 준비한다. 파일 내용의 암호학적 검증은 수행하지 않는다.

## 준비·취소·재개

```bash
bxdl setup --network --workspace ./network-session \
  --from ./instance.json --network-file ./network.json \
  --output ./node.json

# 중단한 동일 세션 재개
bxdl setup --network --workspace ./network-session --resume \
  --output ./node.json
```

터미널에서 실행한다. `--workspace`를 명시하며 새 실행에는 `--from`과 `--network-file`이 모두 필요하다. 재개할 때는 두 원본을 다시 지정하지 않는다. `--install`, `--non-interactive`, `--json`과 혼용하지 않는다. `--output`을 지정해도 저장 전에 명시 확인을 받는다.

도우미는 TLS key/trust store 형식, `new-network` 또는 `join-network`, consensus auto-start, sync archive 사용 여부를 묻고 답마다 기록한다. 요약에서 local identity, 노드 수와 권한·선택 정책을 검토하고, `edit`로 항목 번호 하나를 수정한 뒤 요약으로 돌아간다. 다른 답은 보존한다. 초안만 저장하거나 새 native 파일로 출력할 수 있다.

모든 질문·선택·출력 경로·확인에서 `:help`는 현재 문맥 안내 후 같은 입력으로 돌아가고, `:back`은 앞 질문 또는 이전 검토 단계로 돌아간다. `:cancel`/EOF는 마지막 답을 보존하며 exit 5다. 예약 문자가 실제 파일명인 경우 `./:help`처럼 명시 경로로 입력한다. 새 파일 저장은 `y`/`yes`만 승인하며 빈 Enter는 거절이다. `bxdl setup --network --help`에서 이 모드의 사용법을 확인한다.

- `new-network`: sync를 끈다. 새 네트워크의 공동 초기 구성을 전제로 하며 기존 chain을 따라잡는 설정이 아니다.
- `join-network`: sync=true, GENESIS_VERIFIED로 생성한다. 실제 연결 가능한 peer와 archive source가 필요하다. 입력 성공을 catch-up 성공으로 표시하지 않는다.
- consensus auto-start는 나중에 노드를 시작했을 때의 합의 정책이다. 도우미가 지금 노드를 시작한다는 의미가 아니다.
- archive는 다른 노드의 동기화에 필요한 이력을 저장한다. 자동 GC/control은 끈다. 저장 공간·retention 정책은 별도 운영 과제다.

제품 설정으로 지정한 파일은 참조 검사를 위해 먼저 읽는다. 발견된 키·데이터 경로와 겹치는 network/chain은 내용을 읽기 전에 거부한다.

제품·network·chain의 identity/hash를 고정하므로 원본이 바뀌면 같은 세션의 재개·출력을 거부한다. 원본을 변경한 경우 새 세션에서 다시 검토한다. 인증서·pin 교체는 기존 세션이나 실행 중 노드에 자동 적용되지 않으며 RC.1~RC.3에는 hot reload가 없다.

workspace는 0700, checkpoint/native는 0600이다. native에는 secret **경로**가 들어가므로 공개 파일로 공유하지 않는다. 출력은 workspace 밖의 새 파일이어야 하고 product/network/chain/secret/data 경로와 겹치면 안 된다. 기존 파일을 덮어쓰지 않는다. 게시 도중 I/O나 입력 재검증이 실패하면 `UNKNOWN`/exit 6으로 남기며 출력 파일이 존재할 수 있다. 자동 삭제·재게시하지 말고 출력과 원본을 확인한다. 상대 참조는 제품 파일 위치에서, 문답의 상대 출력은 최초 실행 기준에서 해석한다. 명령줄 `--output`은 해당 명령의 작업 디렉터리 기준이다.

## Catch-up과 정족수 복구의 관측

RC.1~RC.3 공통 snapshot catch-up 계약은 canonical header·상태를 복원하며 과거 transaction body와 ETH receipt 전체를 복원하지 않는다. 뒤처진 노드에서 과거 receipt가 `null`이어도 해당 거래가 미확정이라고 바로 판단하지 않는다. 확정 높이의 block hash/state root와 이력을 가진 노드의 receipt를 함께 확인한다. 재합류 뒤 새 거래는 별도로 확정 여부를 검사한다.

RC.1의 2026-10-20 시험에서는 거래 확정·canonical catch-up·pin 거부를 확인했지만 45초 정족수 부족 후 실시간 복구와 네 노드 재시작 복구는 실패했다. [당시 결과](../results/2026-10-20-macos-qbft-package.md)는 그대로 보존한다.

RC.2의 [2026-10-21 새 Mac package 시험](../results/2026-10-21-macos-rc2-package.md)에서는 두 노드만 남은45초 동안 미확정·라운드 증가를 확인한 뒤 3/4 복귀에서 원 거래가 재제출 없이 확정됐다. 별도로 라운드 차이를 만든 네 노드를 모두 정상 종료하고 같은 DB로 재시작한 뒤 원 거래를 명시적으로 한 번 재제출한 경우도 네 노드 확정과 후속 거래가 통과했다. 이는 새 RC.2 설치의 시험이며 기존 RC.1 설치 upgrade나 자동 재전송 기능이 아니다.

로컬 READY만으로 네트워크 합의나 복구를 판정하지 않는다. 지정 거래의 receipt와 같은 확정 높이의 block hash/state root를 노드별로 대조하고 UNKNOWN은 그대로 보존한다. 실제 고객 네트워크·다중 호스트·장기운영의 복구 SLA로 해석하지 않는다.

RC.3의 [2026-10-22 Mac package 재인수](../results/2026-10-22-macos-rc3-package.md)에서도 같은 archive로 네 validator의 거래·catch-up, 두 정족수 복구 경로와 wrong-pin 거부를 확인했다. 이전 RC.1 실패와 RC.2 성공 기록은 각각 당시 package의 근거이며 최신 결과로 덮어쓰지 않는다.

## 설치로 이어가기

```bash
bxdl setup --install --workspace ./install-session --from ./instance.json
```

NIGO native 입력에 방금 만든 `node.json`을 지정한다. 설치 도우미의 제품 입력에는 저장값을 유지한다. 원본 archive·신뢰 공개키·engine lock은 등록 완료 전까지 보존한다. 새 V2 등록 후에는 이 세 파일의 관리 보관본을 사용하지만, 제품/native·chain·credential 참조는 계속 필요하다. 완료된 설치 세션의 `generated/` 제품 JSON도 binding이 참조하므로 세션을 삭제·이동하지 않는다. 기존 V1 등록은 원본 공급 파일도 계속 필요하며 자동 전환하지 않는다.

별도로 엔진 cold 검사만 하려면 다음을 사용한다.

```bash
bxdl preflight --config ./instance.json --engine-config ./node.json \
  --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java \
  --lock ./trusted-engine.lock.json --allow-development --json
```

생성 성공은 정적 준비 완료다. Cold 성공도 runtime 검사 전이므로 `INCOMPLETE`/exit 5이며 NGVK identity·TLS store 로딩을 확인한다. 실제 TLS1.3 handshake·신뢰/만료·선택된 alias·peer pin·권한·catch-up·4-validator 합의는 이후 실행 인수에서 확인한다. 이 단계는 설치·DB 초기화·READY·합의 성공을 주장하지 않는다.
