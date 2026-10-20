# RC1 네트워크 설정 준비 UX

## 목적과 범위

기존 제품 `instance.json`, 운영자가 승인한 공개 chain JSON, 모든 validator의 공개 네트워크 목록으로 한 노드의 NIGO native `node.json`을 만든다. 운영 시 NIGO 소스나 Gradle, 인터넷은 필요하지 않다. Rust CLI·macOS 우선 방향을 유지하며 기존 설치 도우미에 준비한 파일을 전달한다.

이 기능의 대상은 source `540a89f0814c01d2a4ec67da1087af35195882ca`의 RC1 계약이다. 최신 NIGO main의 속성을 추측하여 추가하지 않는다. 첫 범위는 1~128개 validator로 구성된 전체 목록이며 observer, trusted checkpoint, 고객 CA·키 발급기는 후속이다. 1개 입력을 지원한다는 것은 4-validator 제품 인수를 대신하지 않는다.

## 운영자 흐름

1. 기존 제품 도우미로 로컬 node ID, 주소·port, 승인 chain 및 외부 NGVK/TLS/password 파일 참조를 준비한다.
2. 각 운영자의 공개 leaf 인증서로 `config fingerprint`를 실행한다. NIGO MTLS pin과 공개 node/validator identity·endpoint·거래 권한을 공통 network JSON에 모은다.
3. `setup --network --workspace <새 폴더> --from <제품 JSON> --network-file <공개 목록>`으로 시작한다. 제품 node ID로 로컬 항목을 선택하며 로컬 P2P 주소·port와 일치해야 한다.
4. key/trust store 형식(PKCS12/JKS), 새 네트워크/기존 네트워크 참가, 합의 자동 시작, archive 저장 여부를 선택한다. 각 답을 저장하고 EOF/취소 후 명시 재개한다.
5. 공개 요약을 보고 새 native 출력 경로를 확인한다. 기존 파일을 덮어쓰지 않고 0600으로 게시한다.
6. 기존 `setup --install --from <제품 JSON>`에서 native 입력으로 생성한 파일을 선택한다. 기존 등록·cold·명시 init/start 흐름을 사용한다.

`setup --network`는 터미널 전용이다. 준비와 설치는 각각 별도 workspace를 사용한다. 기존 제품 초안 v1, 설치 Plan/Journal/Prepared 직렬화 형식은 변경하지 않는다.

## 입력·검사

공통 목록에는 schemaVersion=1, kind=BXDL_QBFT_NETWORK와 `nodes`만 허용한다. 각 항목은 nodeId, validatorId, host, port, transactionPermission, credentialFingerprints, 선택 revokedCredentialFingerprints를 가진다. 비밀번호·개인키·임의 native 속성은 받지 않는다.

제품·network·chain을 크기 제한과 파일 동일성 검사로 읽고, identity와 hash를 private checkpoint에 고정한다. 재개·출력 전 변경은 거부한다. network의 ID·주소·pin은 정규 형식, 중복·소유자 충돌·active/revoked 겹침, 전체 chain membership 일치를 검사한다. secret은 참조만 재사용하며 실제 NGVK 복호화·TLS store 로딩은 엔진 cold 검사에 맡긴다.

NIGO pin은 일반 인증서 SHA256이 아니다. `SHA256("NIGO_TRANSPORT_CREDENTIAL/v1" + NUL + "MTLS" + NUL + leaf DER)`의 소문자 hex 앞에 `0x`를 붙인다. 공개 인증서 하나(PEM CERTIFICATE 또는 DER)를 받는다. 이 명령의 DER 구조 확인은 X.509 의미·신뢰·만료·서명 검증이 아니다.

출력과 workspace는 product/network/chain/secret/data 및 서로와 겹치면 안 된다. 기존 path 검사로 symlink·hardlink·Mac 경로 별칭을 거부한다. 세션은 0700, checkpoint 및 native 파일은 0600이다. 원본 private input과 경로를 공개 결과에 싣지 않는다.

## 생성 정책과 검증 의미

native 최상위는 `chainFile`, `dataDirectory`, `backend`, `node`다. `node`는 RC1 flat scalar 속성이며 peer/pin 목록은 연속 인덱스로 표현한다. 기존 승인 chain을 참조하며 chain parameter나 membership를 수정하지 않는다. backend RocksDB, local VALIDATOR/MTLS, 두 store 형식·HTTP/P2P·6개 credential 참조를 명시한다.

local ingress와 각 peer의 거래 권한은 network 목록에서 명시적으로 받는다. 예시의 기본 권한은 NONE이다. 새 네트워크는 sync=false, 기존 네트워크 참가는 sync=true/GENESIS_VERIFIED다. archive 여부와 consensus auto-start는 별도 선택이며 자동 GC/control은 false로 유지한다. join 성공에는 실제 peer 연결과 archive를 제공하는 source가 필요하다. archive를 켜면 저장 공간 관리도 후속 운영 과제다.

저장 성공은 정적 준비 완료다. 엔진 cold는 key identity와 TLS store 로딩을 검사해도 INCOMPLETE이며, TLS handshake·인증서 신뢰/만료·peer pin·catch-up·quorum/finality는 runtime 인수에서 확인한다. RC1은 TLS alias 선택 및 hot reload를 제공하지 않으므로 단일 PrivateKeyEntry 자료를 권장하고 변경은 restart 후 적용한다.

## 인수와 후속

정적 renderer, pin 공식, 잘못된 목록·중복·membership 불일치, 취소/재개·입력 변경·출력 겹침·새 파일 게시·비터미널 거부를 검증한다. 실제 RC1/JRE cold 검사는 별도 결과 기록에 실행 bytes와 범위를 남긴다. 이후 동일 실제 package의 4-validator 거래 확정·재시작/catch-up·인증 실패 거부를 인수한다. G1-M, 안전한 제거, J5 및 Linux/Docker는 이 준비 기능의 완료와 구분한다.
